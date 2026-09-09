use cloudless_core::ports::storage::StoragePort;

pub trait MediaEnv {
    type Storage: StoragePort;

    fn media_storage(&self) -> &Self::Storage;
}
