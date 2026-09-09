use crate::core::ports;

pub trait BlogEnv {
    type Repo: ports::BlogPostRepo;

    fn blog_repo(&self) -> &Self::Repo;
}
