use tedge_config::system_services::SystemService;

#[derive(Copy, Clone, Debug, strum_macros::Display, strum_macros::IntoStaticStr)]
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

    pub fn bridge_config_filename(&self) -> &str {
        match self {
            Self::C8y => crate::bridge::C8Y_CONFIG_FILENAME,
            Self::Aws => crate::bridge::AWS_CONFIG_FILENAME,
            Self::Azure => crate::bridge::AZURE_CONFIG_FILENAME,
        }
    }
}
