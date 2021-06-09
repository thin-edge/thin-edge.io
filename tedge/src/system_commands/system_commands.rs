use super::*;

pub struct SystemdStopService {
    pub service_name: String,
}

pub struct SystemdRestartService {
    pub service_name: String,
}

pub struct SystemdEnableService {
    pub service_name: String,
}

pub struct SystemdDisableService {
    pub service_name: String,
}

pub struct SystemdIsServiceActive {
    pub service_name: String,
}

pub struct SystemdVersion;

impl SystemCommand for SystemdStopService {}
impl SystemCommand for SystemdRestartService {}
impl SystemCommand for SystemdEnableService {}
impl SystemCommand for SystemdDisableService {}
impl SystemCommand for SystemdIsServiceActive {}
impl SystemCommand for SystemdVersion {}
