use crate::core::ports;

pub trait ChunkEnv {
    type Repo: ports::ChunkRepo;

    fn chunk_repo(&self) -> &Self::Repo;
}
