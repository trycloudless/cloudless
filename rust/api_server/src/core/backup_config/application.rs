use crate::core::{
    CoreError, CoreResult,
    backup_config::env::BackupConfigEnv,
    ports::BackupConfigRepo,
    subscription::{application as sub_app, env::{PaymentEnv, SubscriptionEnv}},
};
use api_types::{
    backup_config::{
        CreateBackupConfigRequest, CreateBackupConfigResponse, GetBackupConfigResponse,
        ListAllBackupConfigsResponse, ListBackupConfigWithRemoteStorageResponse,
        RenameBackupConfigRequest, RenameBackupConfigResponse, ToggleBackupConfigRequest,
        ToggleBackupConfigResponse, UpdateCleanupTypeRequest, UpdateCleanupTypeResponse,
        UpdateExclusionConfigRequest, UpdateExclusionConfigResponse,
    },
    subscription::TierUsage,
};
use uuid::Uuid;

pub async fn create<E: BackupConfigEnv + SubscriptionEnv + PaymentEnv>(
    env: E,
    user_id: Uuid,
    payload: CreateBackupConfigRequest,
) -> CoreResult<CreateBackupConfigResponse> {
    let limits = sub_app::get_subscription(&env, user_id).await?.limits;
    let count = env.backup_config_repo().count_for_user(user_id).await?;
    if !TierUsage::new(limits, 0, count).can_add_config() {
        return Err(CoreError::forbidden(
            "backup config limit reached for your current plan",
        ));
    }
    env.backup_config_repo().create(user_id, payload).await
}

pub async fn get_by_id<E: BackupConfigEnv>(
    env: E,
    user_id: Uuid,
    id: Uuid,
) -> CoreResult<GetBackupConfigResponse> {
    let config = env.backup_config_repo().get_by_id(user_id, id).await?;
    Ok(GetBackupConfigResponse { config })
}

pub async fn list_config_with_storage<E: BackupConfigEnv>(
    env: E,
    user_id: Uuid,
    physical_device_id: String,
) -> CoreResult<ListBackupConfigWithRemoteStorageResponse> {
    let list = env
        .backup_config_repo()
        .list_config_with_storage(user_id, &physical_device_id)
        .await?;
    Ok(ListBackupConfigWithRemoteStorageResponse { list })
}

pub async fn list_all<E: BackupConfigEnv>(
    env: E,
    user_id: Uuid,
) -> CoreResult<ListAllBackupConfigsResponse> {
    let list = env.backup_config_repo().list_all_for_user(user_id).await?;
    Ok(ListAllBackupConfigsResponse { list })
}

pub async fn set_active<E: BackupConfigEnv>(
    env: E,
    user_id: Uuid,
    payload: ToggleBackupConfigRequest,
) -> CoreResult<ToggleBackupConfigResponse> {
    env.backup_config_repo()
        .set_active(user_id, payload.id, payload.is_active)
        .await
}

pub async fn rename<E: BackupConfigEnv>(
    env: E,
    user_id: Uuid,
    payload: RenameBackupConfigRequest,
) -> CoreResult<RenameBackupConfigResponse> {
    env.backup_config_repo()
        .rename(user_id, payload.id, payload.display_name)
        .await
}

pub async fn update_cleanup_type<E: BackupConfigEnv>(
    env: E,
    user_id: Uuid,
    payload: UpdateCleanupTypeRequest,
) -> CoreResult<UpdateCleanupTypeResponse> {
    env.backup_config_repo()
        .update_cleanup_type(user_id, payload.id, payload.cleanup_type)
        .await
}

pub async fn update_exclusion_config<E: BackupConfigEnv>(
    env: E,
    user_id: Uuid,
    payload: UpdateExclusionConfigRequest,
) -> CoreResult<UpdateExclusionConfigResponse> {
    env.backup_config_repo()
        .update_exclusion_config(
            user_id,
            payload.id,
            payload.encrypted_exclusion_config,
            payload.exclusion_config_nonce,
        )
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ports::{
        BackupConfigRepo, CreateCheckoutSessionRecord, RecordWebhookEventParams,
        SubscriptionProviderFields, SubscriptionRepo,
    };
    use crate::core::subscription::env::SubscriptionEnv;
    use api_types::{
        backup_config::{BackupConfig, BackupConfigWithRemoteStorage, CleanupType},
        subscription::{
            Subscription, SubscriptionHistoryEntry, SubscriptionStatus, SubscriptionTier,
            TierLimits,
        },
    };
    use async_trait::async_trait;
    use chrono::Utc;

    // ---------------------------------------------------------------------------
    // Mock repos
    // ---------------------------------------------------------------------------

    struct MockBackupConfigRepo {
        config_count: u32,
        fail_ownership: bool,
    }

    #[async_trait]
    impl BackupConfigRepo for MockBackupConfigRepo {
        async fn create(
            &self,
            _user_id: Uuid,
            _config: CreateBackupConfigRequest,
        ) -> CoreResult<CreateBackupConfigResponse> {
            if self.fail_ownership {
                return Err(CoreError::forbidden(
                    "one or more referenced resources do not belong to this account",
                ));
            }
            Ok(CreateBackupConfigResponse { id: Uuid::now_v7() })
        }

        async fn get_by_id(&self, _user_id: Uuid, _id: Uuid) -> CoreResult<BackupConfig> {
            unimplemented!()
        }

        async fn list_config_with_storage(
            &self,
            _user_id: Uuid,
            _physical_device_id: &str,
        ) -> CoreResult<Vec<BackupConfigWithRemoteStorage>> {
            unimplemented!()
        }

        async fn list_all_for_user(
            &self,
            _user_id: Uuid,
        ) -> CoreResult<Vec<BackupConfigWithRemoteStorage>> {
            unimplemented!()
        }

        async fn set_active(
            &self,
            _user_id: Uuid,
            _id: Uuid,
            _is_active: bool,
        ) -> CoreResult<ToggleBackupConfigResponse> {
            unimplemented!()
        }

        async fn rename(
            &self,
            _user_id: Uuid,
            _id: Uuid,
            _display_name: String,
        ) -> CoreResult<RenameBackupConfigResponse> {
            unimplemented!()
        }

        async fn update_cleanup_type(
            &self,
            _user_id: Uuid,
            _id: Uuid,
            _cleanup_type: CleanupType,
        ) -> CoreResult<UpdateCleanupTypeResponse> {
            unimplemented!()
        }

        async fn update_exclusion_config(
            &self,
            _user_id: Uuid,
            _id: Uuid,
            _encrypted_exclusion_config: Vec<u8>,
            _exclusion_config_nonce: Vec<u8>,
        ) -> CoreResult<UpdateExclusionConfigResponse> {
            unimplemented!()
        }

        async fn count_for_user(&self, _user_id: Uuid) -> CoreResult<u32> {
            Ok(self.config_count)
        }
    }

    struct MockSubscriptionRepo {
        tier_limits: TierLimits,
    }

    #[async_trait]
    impl SubscriptionRepo for MockSubscriptionRepo {
        async fn get_by_user_id(&self, _user_id: Uuid) -> CoreResult<Option<Subscription>> {
            Ok(None)
        }

        async fn get_tier_limits(&self, _tier: SubscriptionTier) -> CoreResult<TierLimits> {
            Ok(self.tier_limits.clone())
        }

        async fn get_by_provider_subscription_id(
            &self,
            _id: &str,
        ) -> CoreResult<Option<Subscription>> {
            unimplemented!()
        }

        async fn create(
            &self,
            _user_id: Uuid,
            _tier: SubscriptionTier,
            _status: SubscriptionStatus,
        ) -> CoreResult<Subscription> {
            unimplemented!()
        }

        async fn update_tier_and_status(
            &self,
            _id: Uuid,
            _tier: SubscriptionTier,
            _status: SubscriptionStatus,
            _period_start: Option<chrono::DateTime<Utc>>,
            _period_end: Option<chrono::DateTime<Utc>>,
            _cancel_at_period_end: bool,
        ) -> CoreResult<Subscription> {
            unimplemented!()
        }

        async fn set_provider_ids(
            &self,
            _subscription_id: Uuid,
            _provider_subscription_id: Option<&str>,
            _provider_customer_id: &str,
        ) -> CoreResult<()> {
            unimplemented!()
        }

        async fn update_provider_fields(
            &self,
            _subscription_id: Uuid,
            _provider_status: &str,
            _provider_product_id: Option<&str>,
            _next_billing_date: Option<chrono::DateTime<Utc>>,
            _expires_at: Option<chrono::DateTime<Utc>>,
            _last_webhook_at: Option<chrono::DateTime<Utc>>,
        ) -> CoreResult<()> {
            unimplemented!()
        }

        async fn get_provider_fields(
            &self,
            _subscription_id: Uuid,
        ) -> CoreResult<Option<SubscriptionProviderFields>> {
            unimplemented!()
        }

        async fn record_webhook_event(
            &self,
            _params: RecordWebhookEventParams<'_>,
        ) -> CoreResult<bool> {
            unimplemented!()
        }

        async fn mark_event_processed(
            &self,
            _webhook_id: &str,
            _error: Option<&str>,
        ) -> CoreResult<()> {
            unimplemented!()
        }

        async fn create_checkout_session_record(
            &self,
            _record: CreateCheckoutSessionRecord,
        ) -> CoreResult<()> {
            unimplemented!()
        }

        async fn complete_checkout_session(
            &self,
            _provider_checkout_session_id: &str,
        ) -> CoreResult<()> {
            unimplemented!()
        }

        async fn complete_checkout_session_by_id(&self, _checkout_id: Uuid) -> CoreResult<()> {
            unimplemented!()
        }

        async fn fail_checkout_session_by_id(&self, _checkout_id: Uuid) -> CoreResult<()> {
            unimplemented!()
        }

        async fn get_checkout_session_status(
            &self,
            _checkout_id: Uuid,
            _user_id: Uuid,
        ) -> CoreResult<Option<String>> {
            unimplemented!()
        }

        async fn record_history(
            &self,
            _subscription_id: Uuid,
            _user_id: Uuid,
            _previous_tier: Option<SubscriptionTier>,
            _new_tier: SubscriptionTier,
            _previous_status: Option<SubscriptionStatus>,
            _new_status: SubscriptionStatus,
            _reason: api_types::subscription::TierChangeReason,
            _webhook_id: Option<&str>,
            _metadata: Option<serde_json::Value>,
        ) -> CoreResult<()> {
            unimplemented!()
        }

        async fn get_history(&self, _user_id: Uuid) -> CoreResult<Vec<SubscriptionHistoryEntry>> {
            unimplemented!()
        }

        async fn list_pending_webhook_events(
            &self,
            _min_age_secs: i64,
        ) -> CoreResult<Vec<crate::core::ports::PendingWebhookEvent>> {
            unimplemented!()
        }

        async fn apply_subscription_change_atomic(
            &self,
            _params: crate::core::ports::SubscriptionChangeParams<'_>,
        ) -> CoreResult<()> {
            unimplemented!()
        }
    }

    // ---------------------------------------------------------------------------
    // Mock env
    // ---------------------------------------------------------------------------

    struct MockConfigEnv {
        config_repo: MockBackupConfigRepo,
        subscription_repo: MockSubscriptionRepo,
    }

    impl BackupConfigEnv for MockConfigEnv {
        type Repo = MockBackupConfigRepo;
        fn backup_config_repo(&self) -> &Self::Repo {
            &self.config_repo
        }
    }

    impl SubscriptionEnv for MockConfigEnv {
        type Repo = MockSubscriptionRepo;
        fn subscription_repo(&self) -> &Self::Repo {
            &self.subscription_repo
        }
    }

    // PaymentEnv: tests exercise subscription-limit logic via the DB path,
    // so billing_enabled() must be true (the default). DisabledPaymentProvider
    // satisfies the trait bounds and is never actually called here.
    impl crate::core::subscription::env::PaymentEnv for MockConfigEnv {
        type Provider = crate::infra::billing::disabled::DisabledPaymentProvider;
        type Verifier = crate::infra::billing::disabled::DisabledPaymentProvider;

        fn payment_provider(&self) -> &Self::Provider {
            unimplemented!("not used in backup_config tests")
        }
        fn payment_webhook_verifier(&self) -> &Self::Verifier {
            unimplemented!("not used in backup_config tests")
        }
        fn product_id_for_tier(&self, _: api_types::subscription::SubscriptionTier) -> Option<String> {
            None
        }
        fn payment_environment(&self) -> &str { "" }
        fn return_url_base(&self) -> &str { "" }
        fn cancel_url_base(&self) -> &str { "" }
    }

    fn free_limits() -> TierLimits {
        SubscriptionTier::Free.limits()
    }

    fn unlimited_limits() -> TierLimits {
        SubscriptionTier::Pro.limits()
    }

    fn make_env(config_count: u32, limits: TierLimits) -> MockConfigEnv {
        MockConfigEnv {
            config_repo: MockBackupConfigRepo {
                config_count,
                fail_ownership: false,
            },
            subscription_repo: MockSubscriptionRepo {
                tier_limits: limits,
            },
        }
    }

    fn make_env_with_ownership_violation(config_count: u32, limits: TierLimits) -> MockConfigEnv {
        MockConfigEnv {
            config_repo: MockBackupConfigRepo {
                config_count,
                fail_ownership: true,
            },
            subscription_repo: MockSubscriptionRepo {
                tier_limits: limits,
            },
        }
    }

    fn dummy_create_request() -> CreateBackupConfigRequest {
        CreateBackupConfigRequest {
            storage_id: Uuid::now_v7(),
            local_device_id: Uuid::now_v7(),
            encrypted_source_dir: vec![1, 2, 3],
            source_dir_nonce: vec![4, 5, 6],
            source_dir_blind_index: vec![7, 8, 9],
            display_name: "My Backup".into(),
            cleanup_type: CleanupType::NoCleanup,
            encrypted_exclusion_config: None,
            exclusion_config_nonce: None,
        }
    }

    // ---------------------------------------------------------------------------
    // Tests
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_create_config_under_limit() {
        // Free tier allows 1 config; 0 currently registered → should succeed.
        let env = make_env(0, free_limits());
        let result = create(env, Uuid::now_v7(), dummy_create_request()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_create_config_at_limit_is_rejected() {
        // Free tier allows 1 config; 1 already registered → should be rejected.
        let env = make_env(1, free_limits());
        let err = create(env, Uuid::now_v7(), dummy_create_request())
            .await
            .unwrap_err();
        assert!(matches!(err, CoreError::Forbidden { .. }));
    }

    #[tokio::test]
    async fn test_create_config_pro_tier_no_limit() {
        // Pro tier has no config limit; many configs registered → should succeed.
        let env = make_env(100, unlimited_limits());
        let result = create(env, Uuid::now_v7(), dummy_create_request()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_create_backup_config_propagates_forbidden_on_ownership_violation() {
        // The repo maps a `*_owned_by_user` FK violation (storage_id or local_device_id
        // belonging to another user) to CoreError::Forbidden. The application layer must
        // propagate that error unchanged, not mask it as a generic/internal failure.
        let env = make_env_with_ownership_violation(0, free_limits());
        let err = create(env, Uuid::now_v7(), dummy_create_request())
            .await
            .unwrap_err();
        assert!(matches!(err, CoreError::Forbidden { .. }));
    }
}
