use api_types::subscription::*;
use uuid::Uuid;

use crate::core::{
    CoreError, CoreResult, ports::SubscriptionRepo,
    subscription::env::{PaymentEnv, SubscriptionEnv},
};

/// Get a user's subscription with limits fetched from the database.
///
/// On self-hosted deployments (`billing_enabled = false`) skips the database
/// entirely and returns Pro-equivalent unlimited limits so all features are
/// unlocked without a paid subscription.
pub async fn get_subscription<E: SubscriptionEnv + PaymentEnv>(
    env: &E,
    user_id: Uuid,
) -> CoreResult<SubscriptionResponse> {
    // Self-hosted: return Pro limits without touching the subscriptions table.
    if !env.billing_enabled() {
        return Ok(SubscriptionResponse {
            billing_enabled: false,
            effective_tier: SubscriptionTier::Pro,
            limits: TierLimits {
                max_devices: None,
                max_backup_configs: None,
                auto_backup_enabled: true,
                metadata_retention_days: None,
            },
            ..Subscription::default_free().to_response()
        });
    }

    match env.subscription_repo().get_by_user_id(user_id).await? {
        Some(sub) => {
            let effective_tier = sub.effective_tier();
            let limits = env
                .subscription_repo()
                .get_tier_limits(effective_tier)
                .await?;
            Ok(SubscriptionResponse {
                subscription: sub,
                effective_tier,
                limits,
                billing_enabled: true,
            })
        }
        None => {
            let limits = env
                .subscription_repo()
                .get_tier_limits(SubscriptionTier::Free)
                .await?;
            Ok(SubscriptionResponse {
                subscription: Subscription::default_free(),
                effective_tier: SubscriptionTier::Free,
                limits,
                billing_enabled: true,
            })
        }
    }
}

/// Ensure a subscription exists for a user. Creates a free-tier subscription
/// if none exists. Called during signup.
pub async fn ensure_subscription<E: SubscriptionEnv>(
    env: &E,
    user_id: Uuid,
) -> CoreResult<Subscription> {
    if let Some(sub) = env.subscription_repo().get_by_user_id(user_id).await? {
        return Ok(sub);
    }

    let sub = env
        .subscription_repo()
        .create(user_id, SubscriptionTier::Free, SubscriptionStatus::Active)
        .await?;

    // Record the initial history entry
    env.subscription_repo()
        .record_history(
            sub.id,
            user_id,
            None,
            SubscriptionTier::Free,
            None,
            SubscriptionStatus::Active,
            TierChangeReason::Initial,
            None,
            None,
        )
        .await?;

    Ok(sub)
}

/// Apply a tier/status change to a subscription and record it in history.
///
/// This is the central mutation method — every subscription change (webhook,
/// admin override, plan change) should go through this to ensure the audit
/// trail is always complete.
pub async fn apply_change<E: SubscriptionEnv>(
    env: &E,
    subscription: &Subscription,
    new_tier: SubscriptionTier,
    new_status: SubscriptionStatus,
    reason: TierChangeReason,
    period_start: Option<chrono::DateTime<chrono::Utc>>,
    period_end: Option<chrono::DateTime<chrono::Utc>>,
    cancel_at_period_end: bool,
    external_event_id: Option<&str>,
    metadata: Option<serde_json::Value>,
) -> CoreResult<Subscription> {
    let updated = env
        .subscription_repo()
        .update_tier_and_status(
            subscription.id,
            new_tier,
            new_status,
            period_start,
            period_end,
            cancel_at_period_end,
        )
        .await?;

    env.subscription_repo()
        .record_history(
            subscription.id,
            subscription.user_id,
            Some(subscription.tier),
            new_tier,
            Some(subscription.status),
            new_status,
            reason,
            external_event_id,
            metadata,
        )
        .await?;

    tracing::info!(
        user_id = %subscription.user_id,
        from_tier = %subscription.tier,
        to_tier = %new_tier,
        reason = %reason,
        "Subscription changed"
    );

    Ok(updated)
}

/// Get the full subscription history for a user.
pub async fn get_history<E: SubscriptionEnv>(
    env: &E,
    user_id: Uuid,
) -> CoreResult<SubscriptionHistoryResponse> {
    let entries = env.subscription_repo().get_history(user_id).await?;
    Ok(SubscriptionHistoryResponse { entries })
}

/// Get a subscription by its provider subscription ID.
pub async fn get_by_provider_subscription_id<E: SubscriptionEnv>(
    env: &E,
    provider_subscription_id: &str,
) -> CoreResult<Subscription> {
    env.subscription_repo()
        .get_by_provider_subscription_id(provider_subscription_id)
        .await?
        .ok_or_else(|| {
            CoreError::not_found(format!(
                "Subscription with provider_subscription_id: {}",
                provider_subscription_id
            ))
        })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use uuid::Uuid;

    // ---------------------------------------------------------------------------
    // Mock subscription repo
    // ---------------------------------------------------------------------------

    /// Mock that returns a fixed optional subscription and fixed tier limits.
    struct MockSubscriptionRepo {
        /// None = simulate "no subscription row in DB"; Some = return this subscription.
        existing: Option<Subscription>,
        /// Limits to return for any tier query.
        tier_limits: TierLimits,
    }

    #[async_trait]
    impl crate::core::ports::SubscriptionRepo for MockSubscriptionRepo {
        async fn get_by_user_id(&self, _user_id: Uuid) -> CoreResult<Option<Subscription>> {
            Ok(self.existing.clone())
        }

        async fn get_tier_limits(&self, _tier: SubscriptionTier) -> CoreResult<TierLimits> {
            Ok(self.tier_limits.clone())
        }

        // All other methods are unreachable in these tests.
        async fn get_by_provider_subscription_id(&self, _: &str) -> CoreResult<Option<Subscription>> { unimplemented!() }
        async fn create(&self, _: Uuid, _: SubscriptionTier, _: SubscriptionStatus) -> CoreResult<Subscription> { unimplemented!() }
        async fn update_tier_and_status(&self, _: Uuid, _: SubscriptionTier, _: SubscriptionStatus, _: Option<chrono::DateTime<chrono::Utc>>, _: Option<chrono::DateTime<chrono::Utc>>, _: bool) -> CoreResult<Subscription> { unimplemented!() }
        async fn set_provider_ids(&self, _: Uuid, _: Option<&str>, _: &str) -> CoreResult<()> { unimplemented!() }
        async fn update_provider_fields(&self, _: Uuid, _: &str, _: Option<&str>, _: Option<chrono::DateTime<chrono::Utc>>, _: Option<chrono::DateTime<chrono::Utc>>, _: Option<chrono::DateTime<chrono::Utc>>) -> CoreResult<()> { unimplemented!() }
        async fn get_provider_fields(&self, _: Uuid) -> CoreResult<Option<crate::core::ports::SubscriptionProviderFields>> { unimplemented!() }
        async fn record_webhook_event(&self, _: crate::core::ports::RecordWebhookEventParams<'_>) -> CoreResult<bool> { unimplemented!() }
        async fn mark_event_processed(&self, _: &str, _: Option<&str>) -> CoreResult<()> { unimplemented!() }
        async fn create_checkout_session_record(&self, _: crate::core::ports::CreateCheckoutSessionRecord) -> CoreResult<()> { unimplemented!() }
        async fn complete_checkout_session(&self, _: &str) -> CoreResult<()> { unimplemented!() }
        async fn complete_checkout_session_by_id(&self, _: Uuid) -> CoreResult<()> { unimplemented!() }
        async fn fail_checkout_session_by_id(&self, _: Uuid) -> CoreResult<()> { unimplemented!() }
        async fn get_checkout_session_status(&self, _: Uuid, _: Uuid) -> CoreResult<Option<String>> { unimplemented!() }
        async fn record_history(&self, _: Uuid, _: Uuid, _: Option<SubscriptionTier>, _: SubscriptionTier, _: Option<SubscriptionStatus>, _: SubscriptionStatus, _: TierChangeReason, _: Option<&str>, _: Option<serde_json::Value>) -> CoreResult<()> { unimplemented!() }
        async fn get_history(&self, _: Uuid) -> CoreResult<Vec<SubscriptionHistoryEntry>> { unimplemented!() }
        async fn list_pending_webhook_events(&self, _: i64) -> CoreResult<Vec<crate::core::ports::PendingWebhookEvent>> { unimplemented!() }
        async fn apply_subscription_change_atomic(&self, _: crate::core::ports::SubscriptionChangeParams<'_>) -> CoreResult<()> { unimplemented!() }
    }

    // ---------------------------------------------------------------------------
    // Mock env
    // ---------------------------------------------------------------------------

    /// Combines a subscription repo mock with a billing_enabled flag.
    struct MockSubEnv {
        subscription_repo: MockSubscriptionRepo,
        /// Mirrors `AppEnv::billing_enabled()` — controls the self-hosted early-return path.
        billing_enabled: bool,
    }

    impl crate::core::subscription::env::SubscriptionEnv for MockSubEnv {
        type Repo = MockSubscriptionRepo;
        fn subscription_repo(&self) -> &Self::Repo {
            &self.subscription_repo
        }
    }

    impl crate::core::subscription::env::PaymentEnv for MockSubEnv {
        type Provider = crate::infra::billing::disabled::DisabledPaymentProvider;
        type Verifier = crate::infra::billing::disabled::DisabledPaymentProvider;

        fn payment_provider(&self) -> &Self::Provider { unimplemented!() }
        fn payment_webhook_verifier(&self) -> &Self::Verifier { unimplemented!() }
        fn product_id_for_tier(&self, _: api_types::subscription::SubscriptionTier) -> Option<String> { None }
        fn payment_environment(&self) -> &str { "" }
        fn return_url_base(&self) -> &str { "" }
        fn cancel_url_base(&self) -> &str { "" }
        fn billing_enabled(&self) -> bool { self.billing_enabled }
    }

    // ---------------------------------------------------------------------------
    // Helpers
    // ---------------------------------------------------------------------------

    fn self_hosted_env() -> MockSubEnv {
        // billing_enabled = false → should bypass DB and return Pro limits
        MockSubEnv {
            billing_enabled: false,
            subscription_repo: MockSubscriptionRepo {
                existing: None,
                tier_limits: SubscriptionTier::Free.limits(), // never reached
            },
        }
    }

    fn hosted_env_no_row() -> MockSubEnv {
        // billing_enabled = true, no existing subscription → returns free tier from DB
        MockSubEnv {
            billing_enabled: true,
            subscription_repo: MockSubscriptionRepo {
                existing: None,
                tier_limits: SubscriptionTier::Free.limits(),
            },
        }
    }

    fn hosted_env_pro_row() -> MockSubEnv {
        // billing_enabled = true, existing Pro subscription → returns Pro from DB
        let now = chrono::Utc::now();
        let sub = Subscription {
            id: Uuid::nil(),
            user_id: Uuid::nil(),
            tier: SubscriptionTier::Pro,
            status: SubscriptionStatus::Active,
            current_period_start: Some(now),
            current_period_end: Some(now + chrono::Duration::days(30)),
            cancel_at_period_end: false,
            created_at: now,
            updated_at: now,
        };
        MockSubEnv {
            billing_enabled: true,
            subscription_repo: MockSubscriptionRepo {
                existing: Some(sub),
                tier_limits: SubscriptionTier::Pro.limits(),
            },
        }
    }

    // ---------------------------------------------------------------------------
    // Tests
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_self_hosted_returns_pro_limits_without_db() {
        // Self-hosted: billing_enabled = false. Should skip the DB entirely and
        // return Pro-equivalent unlimited limits with billing_enabled: false.
        // The repo returns None but its get_by_user_id should never be reached.
        let env = self_hosted_env();
        let resp = get_subscription(&env, Uuid::now_v7()).await.unwrap();

        assert!(!resp.billing_enabled, "billing_enabled must be false in self-hosted mode");
        assert_eq!(resp.effective_tier, SubscriptionTier::Pro);
        assert_eq!(resp.limits.max_devices, None, "unlimited devices in self-hosted mode");
        assert_eq!(resp.limits.max_backup_configs, None, "unlimited configs in self-hosted mode");
        assert!(resp.limits.auto_backup_enabled, "auto-backup must be enabled in self-hosted mode");
        assert_eq!(resp.limits.metadata_retention_days, None, "unlimited retention in self-hosted mode");
    }

    #[tokio::test]
    async fn test_hosted_no_subscription_row_returns_free_tier() {
        // Billing active, no DB row → falls back to free-tier with billing_enabled: true.
        let env = hosted_env_no_row();
        let resp = get_subscription(&env, Uuid::now_v7()).await.unwrap();

        assert!(resp.billing_enabled, "billing_enabled must be true for hosted deployment");
        assert_eq!(resp.effective_tier, SubscriptionTier::Free);
        assert_eq!(resp.limits.max_devices, Some(1), "free tier capped at 1 device");
        assert!(!resp.limits.auto_backup_enabled, "auto-backup disabled on free tier");
    }

    #[tokio::test]
    async fn test_hosted_with_subscription_row_returns_pro_tier() {
        // Billing active, Pro subscription in DB → returns Pro limits with billing_enabled: true.
        let env = hosted_env_pro_row();
        let resp = get_subscription(&env, Uuid::now_v7()).await.unwrap();

        assert!(resp.billing_enabled, "billing_enabled must be true for hosted deployment");
        assert_eq!(resp.effective_tier, SubscriptionTier::Pro);
        assert_eq!(resp.limits.max_devices, None, "Pro tier has unlimited devices");
        assert!(resp.limits.auto_backup_enabled, "auto-backup enabled on Pro tier");
    }
}
