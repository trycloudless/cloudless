use crate::core::{CoreResult, chunks::env::ChunkEnv, ports::ChunkRepo};
use api_types::{
    chunk::{CreateChunkRequest, CreateChunkResponse, UpdateChunkStorageMetaRequest},
    restore_file_info::{GetFileVersionChunksRequest, GetFileVersionChunksResponse},
};
use uuid::Uuid;

pub async fn create_chunk<E: ChunkEnv>(
    env: E,
    user_id: Uuid,
    payload: CreateChunkRequest,
) -> CoreResult<CreateChunkResponse> {
    let storage = env.chunk_repo().create(user_id, payload).await?;
    Ok(storage)
}

pub async fn get_chunks_for_version<E: ChunkEnv>(
    env: E,
    user_id: Uuid,
    request: GetFileVersionChunksRequest,
) -> CoreResult<GetFileVersionChunksResponse> {
    env.chunk_repo()
        .get_chunks_for_version(user_id, request)
        .await
}

pub async fn update_chunk_storage_meta<E: ChunkEnv>(
    env: E,
    user_id: Uuid,
    request: UpdateChunkStorageMetaRequest,
) -> CoreResult<()> {
    env.chunk_repo().update_storage_meta(user_id, request).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::CoreError;
    use api_types::chunk::{ChunkStatusWithTime, ChunkStorageMeta, ChunkObjectStoreMeta};
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};

    // ---------------------------------------------------------------------------
    // Mock repo
    // ---------------------------------------------------------------------------

    /// Controls what `create()` returns, so a single mock struct can simulate
    /// both a successful create and a repo-level conflict (idempotency mismatch
    /// or a duplicate chunk_index registered with a different chunk).
    #[derive(Clone)]
    enum CreateBehavior {
        Success,
        Conflict(&'static str),
    }

    #[derive(Clone)]
    struct MockChunkRepo {
        create_behavior: CreateBehavior,
        captured_create: Arc<Mutex<Option<CreateChunkRequest>>>,
        captured_get_chunks: Arc<Mutex<Option<GetFileVersionChunksRequest>>>,
        captured_update_meta: Arc<Mutex<Option<UpdateChunkStorageMetaRequest>>>,
    }

    impl MockChunkRepo {
        fn new(create_behavior: CreateBehavior) -> Self {
            Self {
                create_behavior,
                captured_create: Arc::new(Mutex::new(None)),
                captured_get_chunks: Arc::new(Mutex::new(None)),
                captured_update_meta: Arc::new(Mutex::new(None)),
            }
        }
    }

    #[async_trait]
    impl ChunkRepo for MockChunkRepo {
        async fn create(
            &self,
            _user_id: Uuid,
            chunk: CreateChunkRequest,
        ) -> CoreResult<CreateChunkResponse> {
            match &self.create_behavior {
                CreateBehavior::Success => {
                    let id = Uuid::now_v7();
                    *self.captured_create.lock().unwrap() = Some(chunk);
                    Ok(CreateChunkResponse {
                        id,
                        existed_in_db: false,
                    })
                }
                CreateBehavior::Conflict(message) => Err(CoreError::conflict(*message)),
            }
        }

        async fn get_chunks_for_version(
            &self,
            _user_id: Uuid,
            request: GetFileVersionChunksRequest,
        ) -> CoreResult<GetFileVersionChunksResponse> {
            *self.captured_get_chunks.lock().unwrap() = Some(request);
            Ok(GetFileVersionChunksResponse { chunks: vec![] })
        }

        async fn update_storage_meta(
            &self,
            _user_id: Uuid,
            request: UpdateChunkStorageMetaRequest,
        ) -> CoreResult<()> {
            *self.captured_update_meta.lock().unwrap() = Some(request);
            Ok(())
        }
    }

    // ---------------------------------------------------------------------------
    // Mock env
    // ---------------------------------------------------------------------------

    struct MockChunkEnvImpl {
        repo: MockChunkRepo,
    }

    impl ChunkEnv for MockChunkEnvImpl {
        type Repo = MockChunkRepo;
        fn chunk_repo(&self) -> &Self::Repo {
            &self.repo
        }
    }

    fn dummy_create_request() -> CreateChunkRequest {
        CreateChunkRequest {
            hash: vec![1, 2, 3],
            size: 4096,
            storage_id: Uuid::now_v7(),
            remote_file_version_id: Uuid::now_v7(),
            chunk_index: 0,
            storage_meta: ChunkStorageMeta::ObjectStore(ChunkObjectStoreMeta {
                key: "chunks/abc".to_string(),
                hash: "abc123".to_string(),
                encryption: None,
            }),
            status_history: Vec::<ChunkStatusWithTime>::new(),
            idempotency_key: Uuid::now_v7(),
        }
    }

    // ---------------------------------------------------------------------------
    // Tests
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_create_chunk_delegates_to_repo() {
        // The application layer must forward the request untouched and return
        // whatever the repo produces — no business logic lives here.
        let repo = MockChunkRepo::new(CreateBehavior::Success);
        let env = MockChunkEnvImpl { repo: repo.clone() };
        let user_id = Uuid::now_v7();
        let request = dummy_create_request();
        let expected_index = request.chunk_index;

        let result = create_chunk(env, user_id, request).await;

        assert!(result.is_ok());
        assert!(!result.unwrap().existed_in_db);
        let captured = repo.captured_create.lock().unwrap();
        assert_eq!(
            captured.as_ref().expect("repo.create was not called").chunk_index,
            expected_index
        );
    }

    #[tokio::test]
    async fn test_create_chunk_propagates_conflict() {
        // A conflict from the repo (idempotency-key reuse with a divergent
        // payload, or a chunk_index slot claimed by a different chunk) must
        // reach the caller unchanged — the application layer must not mask it
        // as a generic error.
        let repo = MockChunkRepo::new(CreateBehavior::Conflict(
            "chunk_index already registered with a different chunk for this file version",
        ));
        let env = MockChunkEnvImpl { repo };

        let result = create_chunk(env, Uuid::now_v7(), dummy_create_request()).await;

        match result {
            Err(CoreError::Conflict { message, .. }) => {
                assert_eq!(
                    message,
                    "chunk_index already registered with a different chunk for this file version"
                );
            }
            other => panic!("expected CoreError::Conflict, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_get_chunks_for_version_delegates_to_repo() {
        let repo = MockChunkRepo::new(CreateBehavior::Success);
        let env = MockChunkEnvImpl { repo: repo.clone() };
        let user_id = Uuid::now_v7();
        let request = GetFileVersionChunksRequest {
            version_id: Uuid::now_v7(),
        };

        let result = get_chunks_for_version(env, user_id, request.clone()).await;

        assert!(result.is_ok());
        let captured = repo.captured_get_chunks.lock().unwrap();
        assert_eq!(
            captured.as_ref().expect("repo.get_chunks_for_version was not called").version_id,
            request.version_id
        );
    }

    #[tokio::test]
    async fn test_update_chunk_storage_meta_delegates_to_repo() {
        let repo = MockChunkRepo::new(CreateBehavior::Success);
        let env = MockChunkEnvImpl { repo: repo.clone() };
        let user_id = Uuid::now_v7();
        let request = UpdateChunkStorageMetaRequest {
            chunk_id: Uuid::now_v7(),
            storage_meta: ChunkStorageMeta::ObjectStore(ChunkObjectStoreMeta {
                key: "chunks/def".to_string(),
                hash: "def456".to_string(),
                encryption: None,
            }),
        };

        let result = update_chunk_storage_meta(env, user_id, request.clone()).await;

        assert!(result.is_ok());
        let captured = repo.captured_update_meta.lock().unwrap();
        assert_eq!(
            captured.as_ref().expect("repo.update_storage_meta was not called").chunk_id,
            request.chunk_id
        );
    }
}
