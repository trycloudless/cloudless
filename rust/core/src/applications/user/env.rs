use crate::ports::api::{encrypted_dek_api_port::EncryptedDekApiPort, user_api_port::UserApiPort};

pub trait UserEnv {
    type UserApi: UserApiPort;
    type EncryptedDekApi: EncryptedDekApiPort;

    fn user_api(&self) -> &Self::UserApi;
    fn encrypted_dek_api(&self) -> &Self::EncryptedDekApi;
}
