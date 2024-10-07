use std::borrow::Cow;

use tedge_config::system_services::SystemService;

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

    pub fn bridge_config_filename(&self, profile: Option<&str>) -> Cow<'static, str> {
        match (self, profile) {
            (Self::C8y, None) => crate::bridge::C8Y_CONFIG_FILENAME.into(),
            (Self::C8y, Some(profile)) => format!("c8y_{profile}-bridge.conf").into(),
            (Self::Aws, _) => crate::bridge::AWS_CONFIG_FILENAME.into(),
            (Self::Azure, _) => crate::bridge::AZURE_CONFIG_FILENAME.into(),
        }
    }
}
