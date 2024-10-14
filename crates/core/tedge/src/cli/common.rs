use std::borrow::Cow;

use tedge_config::{system_services::SystemService, ProfileName};

#[derive(Copy, Clone, Debug, strum_macros::Display, strum_macros::IntoStaticStr, PartialEq, Eq)]
pub enum Cloud {
    #[strum(serialize = "Cumulocity")]
    C8y,
    Azure,
    Aws,
}

impl Cloud {
    pub fn mapper_service(&self) -> SystemService {
        match self {
            Cloud::Aws => SystemService::TEdgeMapperAws,
            Cloud::Azure => SystemService::TEdgeMapperAz,
            Cloud::C8y => SystemService::TEdgeMapperC8y,
        }
    }

    pub fn bridge_config_filename(&self, profile: Option<&ProfileName>) -> Cow<'static, str> {
        match (self, profile) {
            (Self::C8y, None) => "c8y-bridge.conf".into(),
            (Self::C8y, Some(profile)) => format!("c8y{profile}-bridge.conf").into(),
            (Self::Aws, None) => "aws-bridge.conf".into(),
            (Self::Aws, Some(profile)) => format!("aws{profile}-bridge.conf").into(),
            (Self::Azure, None) => "az-bridge.conf".into(),
            (Self::Azure, Some(profile)) => format!("az{profile}-bridge.conf").into(),
        }
    }
}
