use agent::SmAgentConfig;
use tedge_config::{
    ConfigRepository, ConfigSettingAccessorStringExt, SoftwarePluginDefaultSetting,
};
use tedge_users::UserManager;

mod agent;
mod error;
mod state;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let user_manager = UserManager::new();

    let tedge_config_location = tedge_config::TEdgeConfigLocation::from_default_system_location();
    let config_repository = tedge_config::TEdgeConfigRepository::new(tedge_config_location);
    let tedge_config = config_repository.load()?;

    initialise_logging();

    let default_plugin_type = tedge_config.query_string_optional(SoftwarePluginDefaultSetting)?;
    let tedge_config_path = config_repository
        .get_config_location()
        .tedge_config_root_path()
        .to_path_buf();
    let sm_agent_config = SmAgentConfig::default()
        .with_default_plugin_type(default_plugin_type)
        .with_sm_home(tedge_config_path);
    let agent = agent::SmAgent::new("tedge_agent", sm_agent_config, user_manager);
    agent.start().await?;
    Ok(())
}

fn initialise_logging() {
    tracing_subscriber::fmt()
        .with_timer(tracing_subscriber::fmt::time::ChronoUtc::with_format(
            "%Y-%m-%dT%H:%M:%S%.3f%:z".into(),
        ))
        .init();
}
