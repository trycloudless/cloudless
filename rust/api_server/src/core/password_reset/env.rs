use crate::core::ports;

pub trait PasswordResetEnv {
    type PasswordResetTokenRepo: ports::PasswordResetTokenRepo;
    type AuthRepo: ports::AuthRepo;
    type UserRepo: ports::UserRepo;
    type EmailTemplateRepo: ports::EmailTemplateRepo;
    type EmailAdapter: ports::EmailPort;

    fn password_reset_token_repo(&self) -> &Self::PasswordResetTokenRepo;
    fn auth_repo(&self) -> &Self::AuthRepo;
    fn user_repo(&self) -> &Self::UserRepo;
    fn email_template_repo(&self) -> &Self::EmailTemplateRepo;
    fn email_adapter(&self) -> &Self::EmailAdapter;
    fn frontend_base_url(&self) -> &str;
}
