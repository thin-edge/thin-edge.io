#[derive(thiserror::Error, Debug)]
pub enum LogRetrievalError {
    #[error(transparent)]
    FromTEdgeConfig(#[from] tedge_config::TEdgeConfigError),

    #[error(transparent)]
    FromConfigSetting(#[from] tedge_config::ConfigSettingError),
}
