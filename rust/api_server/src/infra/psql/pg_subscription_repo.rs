use api_types::subscription::{
    Subscription, SubscriptionHistoryEntry, SubscriptionStatus, SubscriptionTier, TierChangeReason,
    TierLimits,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::Row;
use sqlx::postgres::PgPool;
use uuid::Uuid;

use crate::core::{
    CoreError, CoreResult,
    ports::{
        CreateCheckoutSessionRecord, PendingWebhookEvent, RecordWebhookEventParams,
        SubscriptionChangeParams, SubscriptionProviderFields, SubscriptionRepo,
    },
};

use super::map_sqlx_error;

fn parse_tier(s: &str) -> CoreResult<SubscriptionTier> {
    s.parse::<SubscriptionTier>()
        .map_err(|e| CoreError::internal(e))
}

fn parse_status(s: &str) -> CoreResult<SubscriptionStatus> {
    s.parse::<SubscriptionStatus>()
        .map_err(|e| CoreError::internal(e))
}

fn parse_reason(s: &str) -> CoreResult<TierChangeReason> {
    s.parse::<TierChangeReason>()
        .map_err(|e| CoreError::internal(e))
}

fn map_subscription(r: &sqlx::postgres::PgRow) -> CoreResult<Subscription> {
    Ok(Subscription {
        id: r.get("id"),
        user_id: r.get("user_id"),
        tier: parse_tier(r.get("tier"))?,
        status: parse_status(r.get("status"))?,
        current_period_start: r.get("current_period_start"),
        current_period_end: r.get("current_period_end"),
        cancel_at_period_end: r.get("cancel_at_period_end"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    })
}

#[derive(Clone)]
pub struct PgSubscriptionRepo {
    pub pool: PgPool,
}

impl PgSubscriptionRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

const SELECT_COLS: &str = r#"
    id, user_id, tier::TEXT as tier, status::TEXT as status,
    current_period_start, current_period_end, cancel_at_period_end,
    created_at, updated_at
"#;

#[async_trait]
impl SubscriptionRepo for PgSubscriptionRepo {
    async fn get_by_user_id(&self, user_id: Uuid) -> CoreResult<Option<Subscription>> {
        let row = sqlx::query(&format!(
            "SELECT {} FROM subscriptions WHERE user_id = $1",
            SELECT_COLS
        ))
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        row.as_ref().map(map_subscription).transpose()
    }

    async fn get_by_provider_subscription_id(
        &self,
        provider_subscription_id: &str,
    ) -> CoreResult<Option<Subscription>> {
        let row = sqlx::query(&format!(
            "SELECT {} FROM subscriptions WHERE provider_subscription_id = $1",
            SELECT_COLS
        ))
        .bind(provider_subscription_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        row.as_ref().map(map_subscription).transpose()
    }

    async fn create(
        &self,
        user_id: Uuid,
        tier: SubscriptionTier,
        status: SubscriptionStatus,
    ) -> CoreResult<Subscription> {
        let id = Uuid::now_v7();
        let row = sqlx::query(&format!(
            r#"
            INSERT INTO subscriptions (id, user_id, tier, status)
            VALUES ($1, $2, $3::subscription_tier, $4::subscription_status)
            RETURNING {}
            "#,
            SELECT_COLS
        ))
        .bind(id)
        .bind(user_id)
        .bind(tier.to_string())
        .bind(status.to_string())
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        map_subscription(&row)
    }

    async fn update_tier_and_status(
        &self,
        id: Uuid,
        tier: SubscriptionTier,
        status: SubscriptionStatus,
        period_start: Option<DateTime<Utc>>,
        period_end: Option<DateTime<Utc>>,
        cancel_at_period_end: bool,
    ) -> CoreResult<Subscription> {
        let row = sqlx::query(&format!(
            r#"
            UPDATE subscriptions
            SET tier = $2::subscription_tier,
                status = $3::subscription_status,
                current_period_start = COALESCE($4, current_period_start),
                current_period_end = COALESCE($5, current_period_end),
                cancel_at_period_end = $6,
                updated_at = NOW()
            WHERE id = $1
            RETURNING {}
            "#,
            SELECT_COLS
        ))
        .bind(id)
        .bind(tier.to_string())
        .bind(status.to_string())
        .bind(period_start)
        .bind(period_end)
        .bind(cancel_at_period_end)
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        map_subscription(&row)
    }

    async fn set_provider_ids(
        &self,
        subscription_id: Uuid,
        provider_subscription_id: Option<&str>,
        provider_customer_id: &str,
    ) -> CoreResult<()> {
        // COALESCE preserves the existing provider_subscription_id when None is
        // passed (one-time Lifetime purchases have no recurring subscription ID).
        sqlx::query(
            r#"
            UPDATE subscriptions
            SET provider_subscription_id = COALESCE($2, provider_subscription_id),
                provider_customer_id = $3,
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(subscription_id)
        .bind(provider_subscription_id)
        .bind(provider_customer_id)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(())
    }

    async fn update_provider_fields(
        &self,
        subscription_id: Uuid,
        provider_status: &str,
        provider_product_id: Option<&str>,
        next_billing_date: Option<DateTime<Utc>>,
        expires_at: Option<DateTime<Utc>>,
        last_webhook_at: Option<DateTime<Utc>>,
    ) -> CoreResult<()> {
        sqlx::query(
            r#"
            UPDATE subscriptions
            SET provider_status       = $2,
                provider_product_id   = COALESCE($3, provider_product_id),
                next_billing_date     = COALESCE($4, next_billing_date),
                expires_at            = COALESCE($5, expires_at),
                last_webhook_at       = COALESCE($6, last_webhook_at),
                updated_at            = NOW()
            WHERE id = $1
            "#,
        )
        .bind(subscription_id)
        .bind(provider_status)
        .bind(provider_product_id)
        .bind(next_billing_date)
        .bind(expires_at)
        .bind(last_webhook_at)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(())
    }

    async fn get_provider_fields(
        &self,
        subscription_id: Uuid,
    ) -> CoreResult<Option<SubscriptionProviderFields>> {
        let row = sqlx::query(
            r#"
            SELECT id, provider, provider_subscription_id, provider_customer_id,
                   provider_status, provider_product_id, last_webhook_at, last_synced_at
            FROM subscriptions
            WHERE id = $1
            "#,
        )
        .bind(subscription_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(row.map(|r| SubscriptionProviderFields {
            subscription_id: r.get("id"),
            provider: r.get::<Option<String>, _>("provider").unwrap_or_default(),
            provider_subscription_id: r.get("provider_subscription_id"),
            provider_customer_id: r.get("provider_customer_id"),
            provider_status: r.get("provider_status"),
            provider_product_id: r.get("provider_product_id"),
            last_webhook_at: r.get("last_webhook_at"),
            last_synced_at: r.get("last_synced_at"),
        }))
    }

    /// Insert a webhook event row. Returns `false` if the `webhook_id` already exists
    /// (idempotency: the caller should skip processing and return 200 immediately).
    async fn record_webhook_event(&self, params: RecordWebhookEventParams<'_>) -> CoreResult<bool> {
        let id = Uuid::now_v7();
        let result = sqlx::query(
            r#"
            INSERT INTO subscription_events
                (id, subscription_id, webhook_id, event_type, provider,
                 raw_headers, payload, verification_status, processing_status, processed_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'pending', NULL)
            ON CONFLICT (webhook_id) DO NOTHING
            "#,
        )
        .bind(id)
        .bind(params.subscription_id)
        .bind(params.webhook_id)
        .bind(params.event_type)
        .bind(params.provider)
        .bind(params.raw_headers)
        .bind(params.payload)
        .bind(params.verification_status)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        // rows_affected == 0 means ON CONFLICT hit — duplicate, skip processing
        Ok(result.rows_affected() > 0)
    }

    async fn mark_event_processed(&self, webhook_id: &str, error: Option<&str>) -> CoreResult<()> {
        let status = if error.is_some() { "failed" } else { "done" };
        sqlx::query(
            r#"
            UPDATE subscription_events
            SET processing_status = $2,
                processing_error  = $3,
                processed_at      = NOW()
            WHERE webhook_id = $1
            "#,
        )
        .bind(webhook_id)
        .bind(status)
        .bind(error)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(())
    }

    async fn create_checkout_session_record(
        &self,
        record: CreateCheckoutSessionRecord,
    ) -> CoreResult<()> {
        sqlx::query(
            r#"
            INSERT INTO payment_checkout_sessions
                (id, user_id, tier, provider, provider_checkout_session_id,
                 provider_product_id, status, checkout_url, return_url, cancel_url, metadata)
            VALUES
                ($1, $2, $3::subscription_tier, $4, $5, $6, 'pending', $7, $8, $9, $10)
            "#,
        )
        .bind(record.id)
        .bind(record.user_id)
        .bind(record.tier.to_string())
        .bind(record.provider)
        .bind(record.provider_checkout_session_id)
        .bind(record.provider_product_id)
        .bind(record.checkout_url)
        .bind(record.return_url)
        .bind(record.cancel_url)
        .bind(record.metadata)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(())
    }

    async fn complete_checkout_session(
        &self,
        provider_checkout_session_id: &str,
    ) -> CoreResult<()> {
        sqlx::query(
            r#"
            UPDATE payment_checkout_sessions
            SET status = 'completed', completed_at = NOW()
            WHERE provider_checkout_session_id = $1
            "#,
        )
        .bind(provider_checkout_session_id)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(())
    }

    async fn fail_checkout_session_by_id(&self, checkout_id: Uuid) -> CoreResult<()> {
        let result = sqlx::query(
            r#"
            UPDATE payment_checkout_sessions
            SET status = 'failed', completed_at = NOW()
            WHERE id = $1
              AND status = 'pending'
            "#,
        )
        .bind(checkout_id)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        if result.rows_affected() == 0 {
            tracing::warn!(
                %checkout_id,
                "fail_checkout_session_by_id: no pending checkout row found — may be missing or already terminal"
            );
        }

        Ok(())
    }

    async fn get_checkout_session_status(
        &self,
        checkout_id: Uuid,
        user_id: Uuid,
    ) -> CoreResult<Option<String>> {
        let row = sqlx::query(
            r#"
            SELECT status FROM payment_checkout_sessions
            WHERE id = $1 AND user_id = $2
            "#,
        )
        .bind(checkout_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(row.map(|r| r.get::<String, _>("status")))
    }

    async fn complete_checkout_session_by_id(&self, checkout_id: Uuid) -> CoreResult<()> {
        // No status guard: rows_affected == 0 unambiguously means the ID was not found,
        // letting the caller log a meaningful warning. Already-completed rows get their
        // completed_at refreshed, which is harmless and idempotent.
        let result = sqlx::query(
            r#"
            UPDATE payment_checkout_sessions
            SET status = 'completed', completed_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(checkout_id)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        if result.rows_affected() == 0 {
            tracing::warn!(
                %checkout_id,
                "complete_checkout_session_by_id: no checkout row found for this ID \
                 — metadata may be missing or stale"
            );
        }

        Ok(())
    }

    async fn record_history(
        &self,
        subscription_id: Uuid,
        user_id: Uuid,
        previous_tier: Option<SubscriptionTier>,
        new_tier: SubscriptionTier,
        previous_status: Option<SubscriptionStatus>,
        new_status: SubscriptionStatus,
        reason: TierChangeReason,
        webhook_id: Option<&str>,
        metadata: Option<serde_json::Value>,
    ) -> CoreResult<()> {
        let id = Uuid::now_v7();
        sqlx::query(
            r#"
            INSERT INTO subscription_history
                (id, subscription_id, user_id, previous_tier, new_tier,
                 previous_status, new_status, reason, external_event_id, metadata)
            VALUES
                ($1, $2, $3, $4::subscription_tier, $5::subscription_tier,
                 $6::subscription_status, $7::subscription_status,
                 $8::tier_change_reason, $9, $10)
            ON CONFLICT (subscription_id, external_event_id)
                WHERE external_event_id IS NOT NULL
            DO NOTHING
            "#,
        )
        .bind(id)
        .bind(subscription_id)
        .bind(user_id)
        .bind(previous_tier.map(|t| t.to_string()))
        .bind(new_tier.to_string())
        .bind(previous_status.map(|s| s.to_string()))
        .bind(new_status.to_string())
        .bind(reason.to_string())
        .bind(webhook_id)
        .bind(metadata)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(())
    }

    async fn get_history(&self, user_id: Uuid) -> CoreResult<Vec<SubscriptionHistoryEntry>> {
        let rows = sqlx::query(
            r#"
            SELECT id, subscription_id, user_id,
                   previous_tier::TEXT as previous_tier,
                   new_tier::TEXT as new_tier,
                   previous_status::TEXT as previous_status,
                   new_status::TEXT as new_status,
                   reason::TEXT as reason,
                   external_event_id, metadata,
                   changed_at
            FROM subscription_history
            WHERE user_id = $1
            ORDER BY changed_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        rows.iter()
            .map(|r| {
                let prev_tier: Option<String> = r.get("previous_tier");
                let prev_status: Option<String> = r.get("previous_status");

                Ok(SubscriptionHistoryEntry {
                    id: r.get("id"),
                    subscription_id: r.get("subscription_id"),
                    user_id: r.get("user_id"),
                    previous_tier: prev_tier.as_deref().map(parse_tier).transpose()?,
                    new_tier: parse_tier(r.get("new_tier"))?,
                    previous_status: prev_status.as_deref().map(parse_status).transpose()?,
                    new_status: parse_status(r.get("new_status"))?,
                    reason: parse_reason(r.get("reason"))?,
                    external_event_id: r.get("external_event_id"),
                    metadata: r.get("metadata"),
                    changed_at: r.get("changed_at"),
                })
            })
            .collect()
    }

    async fn get_tier_limits(&self, tier: SubscriptionTier) -> CoreResult<TierLimits> {
        let row = sqlx::query(
            r#"
            SELECT max_devices, max_backup_configs, auto_backup_enabled, metadata_retention_days
            FROM tier_limits
            WHERE tier = $1::subscription_tier
            "#,
        )
        .bind(tier.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        match row {
            Some(r) => Ok(TierLimits {
                max_devices: r.get::<Option<i32>, _>("max_devices").map(|v| v as u32),
                max_backup_configs: r
                    .get::<Option<i32>, _>("max_backup_configs")
                    .map(|v| v as u32),
                auto_backup_enabled: r.get("auto_backup_enabled"),
                metadata_retention_days: r
                    .get::<Option<i32>, _>("metadata_retention_days")
                    .map(|v| v as u32),
            }),
            None => Ok(tier.limits()),
        }
    }

    async fn apply_subscription_change_atomic(
        &self,
        params: SubscriptionChangeParams<'_>,
    ) -> CoreResult<()> {
        let mut tx = self.pool.begin().await.map_err(map_sqlx_error)?;

        let history_id = Uuid::now_v7();

        // Combined UPDATE: tier + status + period + provider fields in one statement.
        sqlx::query(
            r#"
            UPDATE subscriptions
            SET tier                     = $2::subscription_tier,
                status                   = $3::subscription_status,
                current_period_start     = COALESCE($4, current_period_start),
                current_period_end       = COALESCE($5, current_period_end),
                cancel_at_period_end     = $6,
                provider_status          = $7,
                provider_product_id      = COALESCE($8, provider_product_id),
                next_billing_date        = COALESCE($9, next_billing_date),
                expires_at               = COALESCE($10, expires_at),
                last_webhook_at          = COALESCE($11, last_webhook_at),
                last_synced_at           = COALESCE($12, last_synced_at),
                updated_at               = NOW()
            WHERE id = $1
            "#,
        )
        .bind(params.subscription_id)
        .bind(params.new_tier.to_string())
        .bind(params.new_status.to_string())
        .bind(params.period_start)
        .bind(params.period_end)
        .bind(params.cancel_at_period_end)
        .bind(params.provider_status)
        .bind(params.provider_product_id)
        .bind(params.next_billing_date)
        .bind(params.expires_at)
        .bind(params.last_webhook_at)
        .bind(params.last_synced_at)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx_error)?;

        // History insert — idempotent via partial unique index on (subscription_id, external_event_id).
        sqlx::query(
            r#"
            INSERT INTO subscription_history
                (id, subscription_id, user_id, previous_tier, new_tier,
                 previous_status, new_status, reason, external_event_id, metadata)
            VALUES
                ($1, $2, $3, $4::subscription_tier, $5::subscription_tier,
                 $6::subscription_status, $7::subscription_status,
                 $8::tier_change_reason, $9, $10)
            ON CONFLICT (subscription_id, external_event_id)
                WHERE external_event_id IS NOT NULL
            DO NOTHING
            "#,
        )
        .bind(history_id)
        .bind(params.subscription_id)
        .bind(params.user_id)
        .bind(params.previous_tier.map(|t| t.to_string()))
        .bind(params.new_tier.to_string())
        .bind(params.previous_status.map(|s| s.to_string()))
        .bind(params.new_status.to_string())
        .bind(params.reason.to_string())
        .bind(params.webhook_id)
        .bind(params.metadata)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx_error)?;

        tx.commit().await.map_err(map_sqlx_error)?;
        Ok(())
    }

    async fn list_pending_webhook_events(
        &self,
        min_age_secs: i64,
    ) -> CoreResult<Vec<PendingWebhookEvent>> {
        // Atomically claim events by moving them from 'pending'/'recovering' to
        // 'recovering'. FOR UPDATE SKIP LOCKED ensures concurrent workers or a slow
        // original task cannot pick up the same row simultaneously.
        let rows = sqlx::query(
            r#"
            UPDATE subscription_events
            SET processing_status = 'recovering'
            WHERE webhook_id IN (
                SELECT webhook_id
                FROM subscription_events
                WHERE processing_status IN ('pending', 'recovering')
                  AND created_at < NOW() - ($1 * interval '1 second')
                ORDER BY created_at
                LIMIT 50
                FOR UPDATE SKIP LOCKED
            )
            RETURNING webhook_id, event_type, payload
            "#,
        )
        .bind(min_age_secs)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let events = rows
            .iter()
            .map(|row| PendingWebhookEvent {
                webhook_id: row.get("webhook_id"),
                event_type: row.get("event_type"),
                payload: row.get("payload"),
            })
            .collect();

        Ok(events)
    }
}
