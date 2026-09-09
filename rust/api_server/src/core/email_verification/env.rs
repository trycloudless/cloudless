use crate::core::ports;

pub trait EmailVerificationEnv {
    type VerificationCodeRepo: ports::VerificationCodeRepo;
    type AuthRepo: ports::AuthRepo;
    type UserRepo: ports::UserRepo;
    type EmailAdapter: ports::EmailPort;

    fn verification_code_repo(&self) -> &Self::VerificationCodeRepo;
    fn auth_repo(&self) -> &Self::AuthRepo;
    fn user_repo(&self) -> &Self::UserRepo;
    fn email_adapter(&self) -> &Self::EmailAdapter;
}
