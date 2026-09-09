use crate::core::{
    CoreError, CoreResult,
    local_device::env::LocalDeviceEnv,
    ports::LocalDeviceRepo,
    subscription::{application as sub_app, env::{PaymentEnv, SubscriptionEnv}},
};
use api_types::{
    local_device::{
        CreateLocalDeviceRequest, CreateLocalDeviceResponse, GetLocalDeviceByPhysicalIdRequest,
        GetLocalDeviceByPhysicalIdResponse, GetOrCreateLocalDeviceRequest,
        GetOrCreateLocalDeviceResponse, ListAllDevicesResponse, ListDevicesByPlatformRequest,
        ListDevicesByPlatformResponse,
    },
    subscription::TierUsage,
};
use uuid::Uuid;

pub async fn create_local_device<E: LocalDeviceEnv + SubscriptionEnv + PaymentEnv>(
    env: E,
    user_id: Uuid,
    payload: CreateLocalDeviceRequest,
) -> CoreResult<CreateLocalDeviceResponse> {
    let limits = sub_app::get_subscription(&env, user_id).await?.limits;
    let count = env.local_device_repo().count_for_user(user_id).await?;
    if !TierUsage::new(limits, count, 0).can_add_device() {
        return Err(CoreError::forbidden(
            "device limit reached for your current plan",
        ));
    }
    env.local_device_repo().create(user_id, payload).await
}

pub async fn get_by_physical_id<E: LocalDeviceEnv>(
    env: E,
    user_id: Uuid,
    payload: GetLocalDeviceByPhysicalIdRequest,
) -> CoreResult<GetLocalDeviceByPhysicalIdResponse> {
    env.local_device_repo()
        .get_by_physical_id(user_id, &payload.physical_device_id)
        .await
}

pub async fn get_or_create<E: LocalDeviceEnv + SubscriptionEnv + PaymentEnv>(
    env: E,
    user_id: Uuid,
    payload: GetOrCreateLocalDeviceRequest,
) -> CoreResult<GetOrCreateLocalDeviceResponse> {
    // Short-circuit if a matching device is already registered — no limit check needed.
    // See LocalDeviceRepo::find_matching_device for the desktop-vs-mobile matching rules.
    if let Some(existing) = env
        .local_device_repo()
        .find_matching_device(
            user_id,
            &payload.physical_device_id,
            &payload.platform,
            payload.display_name.as_deref(),
        )
        .await?
    {
        return Ok(GetOrCreateLocalDeviceResponse {
            id: existing.id,
            physical_device_id: existing.physical_device_id,
            display_name: existing.display_name,
            platform: existing.platform,
            created_at: existing.created_at,
            was_created: false,
        });
    }
    // New device — enforce tier limit before creating.
    let limits = sub_app::get_subscription(&env, user_id).await?.limits;
    let count = env.local_device_repo().count_for_user(user_id).await?;
    if !TierUsage::new(limits, count, 0).can_add_device() {
        return Err(CoreError::forbidden(
            "device limit reached for your current plan",
        ));
    }
    let created = env
        .local_device_repo()
        .create(
            user_id,
            CreateLocalDeviceRequest {
                physical_device_id: payload.physical_device_id,
                display_name: payload.display_name,
                platform: payload.platform,
            },
        )
        .await?;
    Ok(GetOrCreateLocalDeviceResponse {
        id: created.id,
        physical_device_id: created.physical_device_id,
        display_name: created.display_name,
        platform: created.platform,
        created_at: created.created_at,
        was_created: true,
    })
}

pub async fn list_by_platform<E: LocalDeviceEnv>(
    env: E,
    user_id: Uuid,
    payload: ListDevicesByPlatformRequest,
) -> CoreResult<ListDevicesByPlatformResponse> {
    env.local_device_repo()
        .list_by_platform(user_id, &payload.platform)
        .await
}

pub async fn list_all<E: LocalDeviceEnv>(
    env: E,
    user_id: Uuid,
) -> CoreResult<ListAllDevicesResponse> {
    env.local_device_repo().list_all(user_id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ports::{
        CreateCheckoutSessionRecord, LocalDeviceRepo, RecordWebhookEventParams,
        SubscriptionProviderFields, SubscriptionRepo,
    };
    use crate::core::subscription::env::SubscriptionEnv;
    use api_types::{
        local_device::{ListAllDevicesResponse, ListDevicesByPlatformResponse},
        subscription::{
            Subscription, SubscriptionHistoryEntry, SubscriptionStatus, SubscriptionTier,
            TierLimits,
        },
    };
    use async_trait::async_trait;
    use chrono::Utc;
    use std::sync::{Arc, Mutex};

    // ---------------------------------------------------------------------------
    // Mock repos
    // ---------------------------------------------------------------------------

    /// Captures the arguments a call to `find_matching_device` was made with, so
    /// tests can assert the application layer forwards them unchanged (guards
    /// against reintroducing a field-skipping short-circuit like the original bug).
    type FindMatchingDeviceCall = (Uuid, String, String, Option<String>);

    struct MockLocalDeviceRepo {
        device_count: u32,
        existing_device: Option<GetLocalDeviceByPhysicalIdResponse>,
        last_find_matching_device_call: Arc<Mutex<Option<FindMatchingDeviceCall>>>,
    }

    #[async_trait]
    impl LocalDeviceRepo for MockLocalDeviceRepo {
        async fn create(
            &self,
            _user_id: Uuid,
            req: CreateLocalDeviceRequest,
        ) -> CoreResult<CreateLocalDeviceResponse> {
            Ok(CreateLocalDeviceResponse {
                id: Uuid::now_v7(),
                physical_device_id: req.physical_device_id,
                display_name: req.display_name,
                platform: req.platform,
                created_at: Utc::now(),
            })
        }

        async fn get_by_physical_id(
            &self,
            _user_id: Uuid,
            _physical_device_id: &str,
        ) -> CoreResult<GetLocalDeviceByPhysicalIdResponse> {
            unimplemented!()
        }

        async fn list_by_platform(
            &self,
            _user_id: Uuid,
            _platform: &str,
        ) -> CoreResult<ListDevicesByPlatformResponse> {
            unimplemented!()
        }

        async fn list_all(&self, _user_id: Uuid) -> CoreResult<ListAllDevicesResponse> {
            unimplemented!()
        }

        async fn count_for_user(&self, _user_id: Uuid) -> CoreResult<u32> {
            Ok(self.device_count)
        }

        async fn find_matching_device(
            &self,
            user_id: Uuid,
            physical_device_id: &str,
            platform: &str,
            display_name: Option<&str>,
        ) -> CoreResult<Option<GetLocalDeviceByPhysicalIdResponse>> {
            *self.last_find_matching_device_call.lock().unwrap() = Some((
                user_id,
                physical_device_id.to_string(),
                platform.to_string(),
                display_name.map(|s| s.to_string()),
            ));
            Ok(self.existing_device.clone())
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

    struct MockDeviceEnv {
        device_repo: MockLocalDeviceRepo,
        subscription_repo: MockSubscriptionRepo,
    }

    impl LocalDeviceEnv for MockDeviceEnv {
        type LocalDeviceRepo = MockLocalDeviceRepo;
        fn local_device_repo(&self) -> &Self::LocalDeviceRepo {
            &self.device_repo
        }
    }

    impl SubscriptionEnv for MockDeviceEnv {
        type Repo = MockSubscriptionRepo;
        fn subscription_repo(&self) -> &Self::Repo {
            &self.subscription_repo
        }
    }

    // PaymentEnv: tests exercise subscription-limit logic via the DB path,
    // so billing_enabled() must be true (the default). DisabledPaymentProvider
    // satisfies the trait bounds and is never actually called here.
    impl crate::core::subscription::env::PaymentEnv for MockDeviceEnv {
        type Provider = crate::infra::billing::disabled::DisabledPaymentProvider;
        type Verifier = crate::infra::billing::disabled::DisabledPaymentProvider;

        fn payment_provider(&self) -> &Self::Provider {
            unimplemented!("not used in local_device tests")
        }
        fn payment_webhook_verifier(&self) -> &Self::Verifier {
            unimplemented!("not used in local_device tests")
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

    fn make_env(device_count: u32, limits: TierLimits) -> MockDeviceEnv {
        MockDeviceEnv {
            device_repo: MockLocalDeviceRepo {
                device_count,
                existing_device: None,
                last_find_matching_device_call: Arc::new(Mutex::new(None)),
            },
            subscription_repo: MockSubscriptionRepo {
                tier_limits: limits,
            },
        }
    }

    fn make_env_with_existing(
        device_count: u32,
        limits: TierLimits,
        existing: GetLocalDeviceByPhysicalIdResponse,
    ) -> MockDeviceEnv {
        MockDeviceEnv {
            device_repo: MockLocalDeviceRepo {
                device_count,
                existing_device: Some(existing),
                last_find_matching_device_call: Arc::new(Mutex::new(None)),
            },
            subscription_repo: MockSubscriptionRepo {
                tier_limits: limits,
            },
        }
    }

    fn dummy_existing_device() -> GetLocalDeviceByPhysicalIdResponse {
        GetLocalDeviceByPhysicalIdResponse {
            id: Uuid::now_v7(),
            physical_device_id: "mac-001".into(),
            display_name: Some("MacBook Pro".into()),
            platform: "macos".into(),
            created_at: Utc::now(),
        }
    }

    fn create_req() -> CreateLocalDeviceRequest {
        CreateLocalDeviceRequest {
            physical_device_id: "mac-002".into(),
            display_name: None,
            platform: "macos".into(),
        }
    }

    fn get_or_create_req() -> GetOrCreateLocalDeviceRequest {
        GetOrCreateLocalDeviceRequest {
            physical_device_id: "mac-002".into(),
            display_name: None,
            platform: "macos".into(),
        }
    }

    // ---------------------------------------------------------------------------
    // create_local_device tests
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_create_device_under_limit() {
        // Free tier allows 1 device; 0 currently registered → should succeed.
        let env = make_env(0, free_limits());
        let result = create_local_device(env, Uuid::now_v7(), create_req()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_create_device_at_limit_is_rejected() {
        // Free tier allows 1 device; 1 already registered → should be rejected.
        let env = make_env(1, free_limits());
        let err = create_local_device(env, Uuid::now_v7(), create_req())
            .await
            .unwrap_err();
        assert!(matches!(err, CoreError::Forbidden { .. }));
    }

    #[tokio::test]
    async fn test_create_device_pro_tier_no_limit() {
        // Pro tier has no device limit; many devices registered → should succeed.
        let env = make_env(100, unlimited_limits());
        let result = create_local_device(env, Uuid::now_v7(), create_req()).await;
        assert!(result.is_ok());
    }

    // ---------------------------------------------------------------------------
    // get_or_create tests
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_get_or_create_existing_device_bypasses_limit() {
        // Device already registered: even at the free limit (1/1), get_or_create
        // should short-circuit and return the existing device without checking limits.
        let existing = dummy_existing_device();
        let env = make_env_with_existing(1, free_limits(), existing.clone());
        let result = get_or_create(env, Uuid::now_v7(), get_or_create_req()).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.id, existing.id);
        assert!(!resp.was_created);
    }

    #[tokio::test]
    async fn test_get_or_create_new_device_under_limit() {
        // No existing device; free tier has 0 devices registered → should create.
        let env = make_env(0, free_limits());
        let result = get_or_create(env, Uuid::now_v7(), get_or_create_req()).await;
        assert!(result.is_ok());
        assert!(result.unwrap().was_created);
    }

    #[tokio::test]
    async fn test_get_or_create_new_device_at_limit_is_rejected() {
        // No existing device (find_matching_device returns None); free tier is
        // already at 1/1 → limit check should block the creation.
        let env = make_env(1, free_limits());
        let err = get_or_create(env, Uuid::now_v7(), get_or_create_req())
            .await
            .unwrap_err();
        assert!(matches!(err, CoreError::Forbidden { .. }));
    }

    #[tokio::test]
    async fn test_get_or_create_pro_tier_no_limit() {
        let env = make_env(50, unlimited_limits());
        let result = get_or_create(env, Uuid::now_v7(), get_or_create_req()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_or_create_passes_display_name_and_platform_unchanged_to_matcher() {
        // Regression guard for the original bug: get_or_create used to short-circuit
        // via a lookup that ignored platform/display_name entirely. Assert the exact
        // tuple the caller supplied reaches the repo's find_matching_device call.
        let call_log: Arc<Mutex<Option<FindMatchingDeviceCall>>> = Arc::new(Mutex::new(None));
        let env = MockDeviceEnv {
            device_repo: MockLocalDeviceRepo {
                device_count: 0,
                existing_device: None,
                last_find_matching_device_call: call_log.clone(),
            },
            subscription_repo: MockSubscriptionRepo {
                tier_limits: free_limits(),
            },
        };
        let user_id = Uuid::now_v7();
        let req = GetOrCreateLocalDeviceRequest {
            physical_device_id: "".into(),
            display_name: Some("My iPhone".into()),
            platform: "ios".into(),
        };

        let result = get_or_create(env, user_id, req).await;
        assert!(result.is_ok());

        let captured = call_log.lock().unwrap().clone();
        assert_eq!(
            captured,
            Some((user_id, "".to_string(), "ios".to_string(), Some("My iPhone".to_string())))
        );
    }

    #[tokio::test]
    async fn test_get_or_create_mobile_with_none_display_name_always_creates() {
        // Documents the NULL-never-matches contract at the application layer: a mobile
        // device (empty physical_device_id) with no display_name, and no match found by
        // the repo, must fall through to creating a new device rather than merging into
        // some other nameless device.
        let env = make_env(0, free_limits());
        let req = GetOrCreateLocalDeviceRequest {
            physical_device_id: "".into(),
            display_name: None,
            platform: "ios".into(),
        };

        let result = get_or_create(env, Uuid::now_v7(), req).await;
        assert!(result.is_ok());
        assert!(result.unwrap().was_created);
    }
}
