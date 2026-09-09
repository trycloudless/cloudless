use crate::core::{
    CoreResult,
    ports::RemoteFileVersionRepo,
    remote_file_version::env::RemoteFileVersionEnv,
    subscription::{application as sub_app, env::{PaymentEnv, SubscriptionEnv}},
};
use api_types::remote_file_version::{
    CreateFileVersionRequest, CreateFileVersionResponse, ListAllVersionsRequest,
    ListAllVersionsResponse, ListBackedUpFilesRequest, ListBackedUpFilesResponse,
    ListBinVersionsRequest, ListBinVersionsResponse, MoveAllVersionsToBinRequest,
    MoveVersionToBinRequest, RestoreVersionFromBinRequest, UpdateFileVersionStatusRequest,
};
use chrono::Utc;
use uuid::Uuid;

pub async fn create_remote_file_version<E: RemoteFileVersionEnv>(
    env: E,
    user_id: Uuid,
    payload: CreateFileVersionRequest,
) -> CoreResult<CreateFileVersionResponse> {
    env.remote_file_version_repo()
        .create(user_id, payload)
        .await
}

pub async fn list_backed_up_files<E: RemoteFileVersionEnv>(
    env: E,
    user_id: Uuid,
    request: ListBackedUpFilesRequest,
) -> CoreResult<ListBackedUpFilesResponse> {
    env.remote_file_version_repo()
        .list_backed_up_files(user_id, request)
        .await
}

pub async fn list_all_versions<E: RemoteFileVersionEnv + SubscriptionEnv + PaymentEnv>(
    env: E,
    user_id: Uuid,
    request: ListAllVersionsRequest,
) -> CoreResult<ListAllVersionsResponse> {
    let limits = sub_app::get_subscription(&env, user_id).await?.limits;
    let retention_cutoff = limits
        .metadata_retention_days
        .map(|days| Utc::now() - chrono::Duration::days(days as i64));
    env.remote_file_version_repo()
        .list_all_versions(user_id, request, retention_cutoff)
        .await
}

pub async fn update_file_version_status<E: RemoteFileVersionEnv>(
    env: E,
    request: UpdateFileVersionStatusRequest,
) -> CoreResult<()> {
    env.remote_file_version_repo()
        .update_status(request)
        .await?;
    Ok(())
}

/// Move a single file version to bin (soft-delete).
pub async fn move_version_to_bin<E: RemoteFileVersionEnv>(
    env: E,
    user_id: Uuid,
    request: MoveVersionToBinRequest,
) -> CoreResult<()> {
    env.remote_file_version_repo()
        .move_to_bin(user_id, request.version_id)
        .await
}

/// Move all versions of a file to bin.
pub async fn move_all_versions_to_bin<E: RemoteFileVersionEnv>(
    env: E,
    user_id: Uuid,
    request: MoveAllVersionsToBinRequest,
) -> CoreResult<()> {
    env.remote_file_version_repo()
        .move_all_to_bin(user_id, request)
        .await
}

/// Restore a single file version from bin.
pub async fn restore_version_from_bin<E: RemoteFileVersionEnv>(
    env: E,
    user_id: Uuid,
    request: RestoreVersionFromBinRequest,
) -> CoreResult<()> {
    env.remote_file_version_repo()
        .restore_from_bin(user_id, request.version_id)
        .await
}

/// List file versions currently in bin for a backup config.
pub async fn list_bin_versions<E: RemoteFileVersionEnv>(
    env: E,
    user_id: Uuid,
    request: ListBinVersionsRequest,
) -> CoreResult<ListBinVersionsResponse> {
    env.remote_file_version_repo()
        .list_bin_versions(user_id, request)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ports::{
        CreateCheckoutSessionRecord, RecordWebhookEventParams, RemoteFileVersionRepo,
        SubscriptionProviderFields, SubscriptionRepo,
    };
    use crate::core::subscription::env::SubscriptionEnv;
    use api_types::{
        remote_file_version::{
            CreateFileVersionResponse, ListAllVersionsResponse, ListBackedUpFilesResponse,
            ListBinVersionsResponse, MoveAllVersionsToBinRequest, UpdateFileVersionStatusRequest,
        },
        subscription::{
            Subscription, SubscriptionHistoryEntry, SubscriptionStatus, SubscriptionTier,
            TierLimits,
        },
    };
    use async_trait::async_trait;
    use chrono::DateTime;
    use chrono::Utc;
    use std::sync::{Arc, Mutex};

    // ---------------------------------------------------------------------------
    // Mock repos
    // ---------------------------------------------------------------------------

    /// Controls what `create()` returns, so a single mock struct can simulate
    /// both a successful create and either flavor of repo-level conflict —
    /// an idempotency-key reused with a divergent payload, or a stale
    /// `base_version` — which share the same `CoreError::Conflict` wire path
    /// but carry different messages.
    #[derive(Clone)]
    enum CreateBehavior {
        Success,
        Conflict(&'static str),
    }

    #[derive(Clone)]
    struct MockRemoteFileVersionRepo {
        captured_cutoff: Arc<Mutex<Option<Option<DateTime<Utc>>>>>,
        captured_create_base_version: Arc<Mutex<Option<Option<u32>>>>,
        captured_create_idempotency_key: Arc<Mutex<Option<Uuid>>>,
        create_behavior: CreateBehavior,
    }

    impl MockRemoteFileVersionRepo {
        fn new() -> Self {
            Self {
                captured_cutoff: Arc::new(Mutex::new(None)),
                captured_create_base_version: Arc::new(Mutex::new(None)),
                captured_create_idempotency_key: Arc::new(Mutex::new(None)),
                create_behavior: CreateBehavior::Success,
            }
        }

        fn new_with_create_behavior(create_behavior: CreateBehavior) -> Self {
            Self {
                create_behavior,
                ..Self::new()
            }
        }

        fn captured(&self) -> Option<Option<DateTime<Utc>>> {
            *self.captured_cutoff.lock().unwrap()
        }

        fn captured_is_some_some(&self) -> bool {
            matches!(self.captured(), Some(Some(_)))
        }

        fn captured_is_some_none(&self) -> bool {
            matches!(self.captured(), Some(None))
        }
    }

    #[async_trait]
    impl RemoteFileVersionRepo for MockRemoteFileVersionRepo {
        async fn create(
            &self,
            _user_id: Uuid,
            request: CreateFileVersionRequest,
        ) -> CoreResult<CreateFileVersionResponse> {
            *self.captured_create_base_version.lock().unwrap() = Some(request.base_version);
            *self.captured_create_idempotency_key.lock().unwrap() = Some(request.idempotency_key);
            match &self.create_behavior {
                CreateBehavior::Success => Ok(CreateFileVersionResponse {
                    id: Uuid::now_v7(),
                    version: request.base_version.map_or(1, |v| v + 1),
                }),
                CreateBehavior::Conflict(message) => {
                    Err(crate::core::CoreError::conflict(*message))
                }
            }
        }

        async fn list_backed_up_files(
            &self,
            _user_id: Uuid,
            _request: ListBackedUpFilesRequest,
        ) -> CoreResult<ListBackedUpFilesResponse> {
            unimplemented!()
        }

        async fn list_all_versions(
            &self,
            _user_id: Uuid,
            _request: ListAllVersionsRequest,
            retention_cutoff: Option<DateTime<Utc>>,
        ) -> CoreResult<ListAllVersionsResponse> {
            *self.captured_cutoff.lock().unwrap() = Some(retention_cutoff);
            Ok(ListAllVersionsResponse { versions: vec![] })
        }

        async fn update_status(&self, _request: UpdateFileVersionStatusRequest) -> CoreResult<()> {
            unimplemented!()
        }

        async fn move_to_bin(&self, _user_id: Uuid, _version_id: Uuid) -> CoreResult<()> {
            unimplemented!()
        }

        async fn move_all_to_bin(
            &self,
            _user_id: Uuid,
            _request: MoveAllVersionsToBinRequest,
        ) -> CoreResult<()> {
            unimplemented!()
        }

        async fn restore_from_bin(&self, _user_id: Uuid, _version_id: Uuid) -> CoreResult<()> {
            unimplemented!()
        }

        async fn list_bin_versions(
            &self,
            _user_id: Uuid,
            _request: ListBinVersionsRequest,
        ) -> CoreResult<ListBinVersionsResponse> {
            unimplemented!()
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

    struct MockVersionEnv {
        version_repo: MockRemoteFileVersionRepo,
        subscription_repo: MockSubscriptionRepo,
    }

    impl RemoteFileVersionEnv for MockVersionEnv {
        type Repo = MockRemoteFileVersionRepo;
        fn remote_file_version_repo(&self) -> &Self::Repo {
            &self.version_repo
        }
    }

    impl SubscriptionEnv for MockVersionEnv {
        type Repo = MockSubscriptionRepo;
        fn subscription_repo(&self) -> &Self::Repo {
            &self.subscription_repo
        }
    }

    // PaymentEnv: tests exercise subscription-limit logic via the DB path,
    // so billing_enabled() must be true (the default). DisabledPaymentProvider
    // satisfies the trait bounds and is never actually called here.
    impl crate::core::subscription::env::PaymentEnv for MockVersionEnv {
        type Provider = crate::infra::billing::disabled::DisabledPaymentProvider;
        type Verifier = crate::infra::billing::disabled::DisabledPaymentProvider;

        fn payment_provider(&self) -> &Self::Provider {
            unimplemented!("not used in remote_file_version tests")
        }
        fn payment_webhook_verifier(&self) -> &Self::Verifier {
            unimplemented!("not used in remote_file_version tests")
        }
        fn product_id_for_tier(&self, _: api_types::subscription::SubscriptionTier) -> Option<String> {
            None
        }
        fn payment_environment(&self) -> &str { "" }
        fn return_url_base(&self) -> &str { "" }
        fn cancel_url_base(&self) -> &str { "" }
    }

    fn make_env(limits: TierLimits) -> (MockRemoteFileVersionRepo, MockVersionEnv) {
        make_env_with_repo(MockRemoteFileVersionRepo::new(), limits)
    }

    fn make_env_with_repo(
        repo: MockRemoteFileVersionRepo,
        limits: TierLimits,
    ) -> (MockRemoteFileVersionRepo, MockVersionEnv) {
        let env = MockVersionEnv {
            version_repo: repo.clone(),
            subscription_repo: MockSubscriptionRepo {
                tier_limits: limits,
            },
        };
        (repo, env)
    }

    fn dummy_list_request() -> ListAllVersionsRequest {
        ListAllVersionsRequest {
            backup_config_id: Uuid::now_v7(),
        }
    }

    fn dummy_create_request() -> CreateFileVersionRequest {
        CreateFileVersionRequest {
            backup_config_id: Uuid::now_v7(),
            device_id: Uuid::now_v7(),
            storage_id: Uuid::now_v7(),
            size: 1024,
            status: api_types::remote_file_version::FileVersionStatus::Uploading,
            local_file_updated_at: Utc::now(),
            base_version: Some(3),
            idempotency_key: Uuid::now_v7(),
            encrypted_name: vec![1, 2, 3],
            name_nonce: vec![4, 5, 6],
            name_blind_index: vec![7, 8, 9],
        }
    }

    // ---------------------------------------------------------------------------
    // Tests
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_list_all_versions_free_tier_passes_cutoff_timestamp() {
        // Free tier has metadata_retention_days = Some(7) → cutoff = now - 7 days.
        let (repo, env) = make_env(SubscriptionTier::Free.limits());
        let result = list_all_versions(env, Uuid::now_v7(), dummy_list_request()).await;
        assert!(result.is_ok());
        // Captured value is Some(Some(timestamp)) — presence of a cutoff confirms filter is active.
        assert!(
            repo.captured_is_some_some(),
            "Expected a cutoff timestamp for free tier"
        );
    }

    #[tokio::test]
    async fn test_list_all_versions_paid_tier_passes_none() {
        // Pro tier has metadata_retention_days = None (unlimited) → no filter.
        let (repo, env) = make_env(SubscriptionTier::Pro.limits());
        let result = list_all_versions(env, Uuid::now_v7(), dummy_list_request()).await;
        assert!(result.is_ok());
        assert!(
            repo.captured_is_some_none(),
            "Expected None cutoff for pro tier"
        );
    }

    #[tokio::test]
    async fn test_list_all_versions_starter_tier_passes_none() {
        let (repo, env) = make_env(SubscriptionTier::Starter.limits());
        let result = list_all_versions(env, Uuid::now_v7(), dummy_list_request()).await;
        assert!(result.is_ok());
        assert!(
            repo.captured_is_some_none(),
            "Expected None cutoff for starter tier"
        );
    }

    #[tokio::test]
    async fn test_create_remote_file_version_delegates_to_repo() {
        // The application layer must forward base_version/idempotency_key
        // untouched and hand back exactly what the repo produced.
        let (repo, env) = make_env(SubscriptionTier::Pro.limits());
        let user_id = Uuid::now_v7();
        let request = dummy_create_request();
        let expected_key = request.idempotency_key;

        let result = create_remote_file_version(env, user_id, request).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().version, 4); // base_version Some(3) + 1
        assert_eq!(
            *repo.captured_create_base_version.lock().unwrap(),
            Some(Some(3))
        );
        assert_eq!(
            *repo.captured_create_idempotency_key.lock().unwrap(),
            Some(expected_key)
        );
    }

    #[tokio::test]
    async fn test_create_remote_file_version_propagates_idempotency_conflict() {
        // Same idempotency key reused with a divergent payload — the repo
        // signals this with CoreError::Conflict, and it must reach the
        // caller unchanged, distinguishable by message from a base_version
        // conflict below even though both use the same 409 wire path.
        let repo = MockRemoteFileVersionRepo::new_with_create_behavior(CreateBehavior::Conflict(
            "idempotency key already used with a different request",
        ));
        let (_, env) = make_env_with_repo(repo, SubscriptionTier::Pro.limits());

        let result = create_remote_file_version(env, Uuid::now_v7(), dummy_create_request()).await;

        match result {
            Err(crate::core::CoreError::Conflict { message, .. }) => {
                assert_eq!(
                    message,
                    "idempotency key already used with a different request"
                );
            }
            other => panic!("expected CoreError::Conflict, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_create_remote_file_version_propagates_base_version_conflict() {
        // A stale base_version (someone else's write landed first) also maps
        // to CoreError::Conflict, but with a distinct message from the
        // idempotency-mismatch case above.
        let repo = MockRemoteFileVersionRepo::new_with_create_behavior(CreateBehavior::Conflict(
            "stale base_version",
        ));
        let (_, env) = make_env_with_repo(repo, SubscriptionTier::Pro.limits());

        let result = create_remote_file_version(env, Uuid::now_v7(), dummy_create_request()).await;

        match result {
            Err(crate::core::CoreError::Conflict { message, .. }) => {
                assert_eq!(message, "stale base_version");
            }
            other => panic!("expected CoreError::Conflict, got {other:?}"),
        }
    }
}
