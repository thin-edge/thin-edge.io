use tedge_users::UserManager;

mod agent;
mod component;
mod error;
mod state;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let user_manager = UserManager::new();

    let tedge_config_location = tedge_config::TEdgeConfigLocation::from_default_system_location();

    initialise_logging();

    let component = agent::SmAgent::new("tedge_agent", user_manager, tedge_config_location);
    component.start().await?;
    Ok(())
}

fn initialise_logging() {
    tracing_subscriber::fmt()
        .with_timer(tracing_subscriber::fmt::time::ChronoUtc::with_format(
            "%Y-%m-%dT%H:%M:%S%.3f%:z".into(),
        ))
        .init();
}
