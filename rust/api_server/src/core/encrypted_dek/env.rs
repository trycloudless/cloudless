use crate::core::ports;

pub trait EncryptedDekEnv {
    type Repo: ports::EncryptedDekRepo;

    fn encrypted_dek_repo(&self) -> &Self::Repo;
}
