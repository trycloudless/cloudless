use crate::model::{base::AppResult, file::ObjectKey};
use async_trait::async_trait;

#[async_trait]
pub trait StoragePort: Send + Sync {
    async fn put(&self, key: &ObjectKey, data: Vec<u8>) -> AppResult<()>;
    async fn get(&self, key: &ObjectKey) -> AppResult<Vec<u8>>;
    async fn delete(&self, key: &ObjectKey) -> AppResult<()>;
    async fn list(&self, prefix: &ObjectKey) -> AppResult<Vec<ObjectKey>>;
    async fn exists(&self, key: &ObjectKey) -> AppResult<bool>;
}

#[async_trait]
impl StoragePort for Box<dyn StoragePort> {
    async fn put(&self, key: &ObjectKey, data: Vec<u8>) -> AppResult<()> {
        self.as_ref().put(key, data).await
    }

    async fn get(&self, key: &ObjectKey) -> AppResult<Vec<u8>> {
        self.as_ref().get(key).await
    }

    async fn delete(&self, key: &ObjectKey) -> AppResult<()> {
        self.as_ref().delete(key).await
    }

    async fn list(&self, prefix: &ObjectKey) -> AppResult<Vec<ObjectKey>> {
        self.as_ref().list(prefix).await
    }

    async fn exists(&self, key: &ObjectKey) -> AppResult<bool> {
        self.as_ref().exists(key).await
    }
}
