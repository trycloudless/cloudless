use crate::core::ports;

pub trait GcEnv {
    type GcRepo: ports::GcRepo;

    fn gc_repo(&self) -> &Self::GcRepo;
}
