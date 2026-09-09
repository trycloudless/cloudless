use crate::core::{CoreResult, gc::env::GcEnv, ports::GcRepo};
use api_types::gc::{
    ConfirmChunkDeletionsRequest, ConfirmChunkDeletionsResponse, GcCollectResponse,
    GcRunDetailResponse, GetGcRunDetailRequest, GetRetentionSettingsResponse, ListGcRunsRequest,
    ListGcRunsResponse, UpdateRetentionSettingsRequest,
};
use uuid::Uuid;

/// Phase A: Mark expired MovedToBin versions as Deleted, identify orphaned chunks.
pub async fn collect<E: GcEnv>(env: E, user_id: Uuid) -> CoreResult<GcCollectResponse> {
    env.gc_repo().collect_expired_versions(user_id).await
}

/// Phase C: Confirm that orphaned chunks have been deleted from storage.
pub async fn confirm_chunk_deletions<E: GcEnv>(
    env: E,
    user_id: Uuid,
    request: ConfirmChunkDeletionsRequest,
) -> CoreResult<ConfirmChunkDeletionsResponse> {
    env.gc_repo()
        .confirm_chunk_deletions(user_id, request)
        .await
}

/// List recent GC runs for a user.
pub async fn list_gc_runs<E: GcEnv>(
    env: E,
    user_id: Uuid,
    request: ListGcRunsRequest,
) -> CoreResult<ListGcRunsResponse> {
    let limit = request.limit.unwrap_or(10).min(100);
    env.gc_repo().list_gc_runs(user_id, limit).await
}

/// Get detailed info for a specific GC run.
pub async fn get_gc_run_detail<E: GcEnv>(
    env: E,
    user_id: Uuid,
    request: GetGcRunDetailRequest,
) -> CoreResult<GcRunDetailResponse> {
    env.gc_repo()
        .get_gc_run_detail(user_id, request.gc_run_id)
        .await
}

/// Get the user's retention settings.
pub async fn get_retention_settings<E: GcEnv>(
    env: E,
    user_id: Uuid,
) -> CoreResult<GetRetentionSettingsResponse> {
    env.gc_repo().get_retention_settings(user_id).await
}

/// Update the user's bin retention period.
pub async fn update_retention_settings<E: GcEnv>(
    env: E,
    user_id: Uuid,
    request: UpdateRetentionSettingsRequest,
) -> CoreResult<()> {
    if request.bin_retention_days < 7 || request.bin_retention_days > 30 {
        return Err(crate::core::CoreError::validation(
            "bin_retention_days must be between 7 and 30",
        ));
    }
    env.gc_repo()
        .update_retention_settings(user_id, request.bin_retention_days)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{CoreError, ports::GcRepo};
    use api_types::gc::{GcRunDetailResponse, GcRunStatus, GcRunSummary};
    use async_trait::async_trait;
    use chrono::Utc;
    use std::sync::{Arc, Mutex};

    // ---------------------------------------------------------------------------
    // Mock repo — records the arguments each call was made with, so delegate tests
    // can assert the application layer forwards user_id/request through unchanged.
    // ---------------------------------------------------------------------------

    #[derive(Default)]
    struct CallLog {
        collect_user_id: Option<Uuid>,
        confirm_call: Option<(Uuid, ConfirmChunkDeletionsRequest)>,
        list_call: Option<(Uuid, i32)>,
        get_retention_call: Option<Uuid>,
        update_retention_call: Option<(Uuid, i32)>,
    }

    struct MockGcRepo {
        log: Arc<Mutex<CallLog>>,
    }

    #[async_trait]
    impl GcRepo for MockGcRepo {
        async fn collect_expired_versions(&self, user_id: Uuid) -> CoreResult<GcCollectResponse> {
            self.log.lock().unwrap().collect_user_id = Some(user_id);
            Ok(GcCollectResponse {
                gc_run_id: Uuid::now_v7(),
                versions_deleted: 3,
                orphaned_chunks: vec![],
            })
        }

        async fn confirm_chunk_deletions(
            &self,
            user_id: Uuid,
            request: ConfirmChunkDeletionsRequest,
        ) -> CoreResult<ConfirmChunkDeletionsResponse> {
            self.log.lock().unwrap().confirm_call = Some((user_id, request));
            Ok(ConfirmChunkDeletionsResponse {
                chunks_deleted: 2,
                storage_freed_bytes: 1024,
            })
        }

        async fn list_gc_runs(&self, user_id: Uuid, limit: i32) -> CoreResult<ListGcRunsResponse> {
            self.log.lock().unwrap().list_call = Some((user_id, limit));
            Ok(ListGcRunsResponse { runs: vec![] })
        }

        async fn get_gc_run_detail(
            &self,
            _user_id: Uuid,
            gc_run_id: Uuid,
        ) -> CoreResult<GcRunDetailResponse> {
            Ok(GcRunDetailResponse {
                run: GcRunSummary {
                    id: gc_run_id,
                    status: GcRunStatus::Completed,
                    versions_deleted: 0,
                    chunks_deleted: 0,
                    storage_freed_bytes: 0,
                    error_message: None,
                    started_at: Utc::now(),
                    completed_at: None,
                },
                versions: vec![],
                chunks: vec![],
            })
        }

        async fn get_retention_settings(
            &self,
            user_id: Uuid,
        ) -> CoreResult<GetRetentionSettingsResponse> {
            self.log.lock().unwrap().get_retention_call = Some(user_id);
            Ok(GetRetentionSettingsResponse {
                bin_retention_days: 30,
            })
        }

        async fn update_retention_settings(
            &self,
            user_id: Uuid,
            bin_retention_days: i32,
        ) -> CoreResult<()> {
            self.log.lock().unwrap().update_retention_call = Some((user_id, bin_retention_days));
            Ok(())
        }
    }

    struct MockGcEnv {
        repo: MockGcRepo,
    }

    impl GcEnv for MockGcEnv {
        type GcRepo = MockGcRepo;
        fn gc_repo(&self) -> &Self::GcRepo {
            &self.repo
        }
    }

    fn make_env() -> (MockGcEnv, Arc<Mutex<CallLog>>) {
        let log = Arc::new(Mutex::new(CallLog::default()));
        (
            MockGcEnv {
                repo: MockGcRepo { log: log.clone() },
            },
            log,
        )
    }

    // ---------------------------------------------------------------------------
    // Delegate tests
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_collect_delegates_to_repo_with_user_id() {
        let (env, log) = make_env();
        let user_id = Uuid::now_v7();
        let result = collect(env, user_id).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().versions_deleted, 3);
        assert_eq!(log.lock().unwrap().collect_user_id, Some(user_id));
    }

    #[tokio::test]
    async fn test_confirm_chunk_deletions_delegates_to_repo() {
        let (env, log) = make_env();
        let user_id = Uuid::now_v7();
        let request = ConfirmChunkDeletionsRequest {
            gc_run_id: Uuid::now_v7(),
            chunk_ids: vec![Uuid::now_v7()],
        };
        let result = confirm_chunk_deletions(env, user_id, request.clone()).await;
        assert!(result.is_ok());
        let (logged_user, logged_req) = log.lock().unwrap().confirm_call.clone().unwrap();
        assert_eq!(logged_user, user_id);
        assert_eq!(logged_req.gc_run_id, request.gc_run_id);
        assert_eq!(logged_req.chunk_ids, request.chunk_ids);
    }

    #[tokio::test]
    async fn test_get_retention_settings_delegates_to_repo() {
        let (env, log) = make_env();
        let user_id = Uuid::now_v7();
        let result = get_retention_settings(env, user_id).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().bin_retention_days, 30);
        assert_eq!(log.lock().unwrap().get_retention_call, Some(user_id));
    }

    // ---------------------------------------------------------------------------
    // list_gc_runs clamp regression
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_list_gc_runs_clamps_limit_to_100() {
        // Guards the existing `.min(100)` clamp against silent removal — a client
        // requesting an enormous limit must not be able to force an unbounded query.
        let (env, log) = make_env();
        let user_id = Uuid::now_v7();
        let request = ListGcRunsRequest {
            limit: Some(10_000),
        };
        let result = list_gc_runs(env, user_id, request).await;
        assert!(result.is_ok());
        assert_eq!(log.lock().unwrap().list_call, Some((user_id, 100)));
    }

    // ---------------------------------------------------------------------------
    // update_retention_settings range validation (CLAUDE.md §6: 7-30 days)
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_update_retention_settings_rejects_below_minimum() {
        // 6 days is one below the documented 7-day floor — must be rejected.
        let (env, _log) = make_env();
        let request = UpdateRetentionSettingsRequest {
            bin_retention_days: 6,
        };
        let err = update_retention_settings(env, Uuid::now_v7(), request)
            .await
            .unwrap_err();
        assert!(matches!(err, CoreError::Validation { .. }));
    }

    #[tokio::test]
    async fn test_update_retention_settings_accepts_minimum_boundary() {
        // 7 days is the documented floor — must be accepted.
        let (env, log) = make_env();
        let user_id = Uuid::now_v7();
        let request = UpdateRetentionSettingsRequest {
            bin_retention_days: 7,
        };
        let result = update_retention_settings(env, user_id, request).await;
        assert!(result.is_ok());
        assert_eq!(log.lock().unwrap().update_retention_call, Some((user_id, 7)));
    }

    #[tokio::test]
    async fn test_update_retention_settings_accepts_maximum_boundary() {
        // 30 days is the documented ceiling — must be accepted.
        let (env, log) = make_env();
        let user_id = Uuid::now_v7();
        let request = UpdateRetentionSettingsRequest {
            bin_retention_days: 30,
        };
        let result = update_retention_settings(env, user_id, request).await;
        assert!(result.is_ok());
        assert_eq!(
            log.lock().unwrap().update_retention_call,
            Some((user_id, 30))
        );
    }

    #[tokio::test]
    async fn test_update_retention_settings_rejects_above_maximum() {
        // 31 days is one above the documented 30-day ceiling — must be rejected.
        let (env, _log) = make_env();
        let request = UpdateRetentionSettingsRequest {
            bin_retention_days: 31,
        };
        let err = update_retention_settings(env, Uuid::now_v7(), request)
            .await
            .unwrap_err();
        assert!(matches!(err, CoreError::Validation { .. }));
    }
}
