use crate::core::ports;

pub trait LocalDeviceEnv {
    type LocalDeviceRepo: ports::LocalDeviceRepo;

    fn local_device_repo(&self) -> &Self::LocalDeviceRepo;
}
