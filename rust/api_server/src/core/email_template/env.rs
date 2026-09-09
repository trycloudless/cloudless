use crate::core::ports;

pub trait EmailTemplateEnv {
    type Repo: ports::EmailTemplateRepo;

    fn email_template_repo(&self) -> &Self::Repo;
}
