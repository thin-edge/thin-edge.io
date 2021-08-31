use agent::SmAgentConfig;

mod agent;
mod error;
mod state;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    initialise_logging();
    let tedge_config_location = tedge_config::TEdgeConfigLocation::from_default_system_location();
    let agent = agent::SmAgent::new(
        "tedge_agent",
        SmAgentConfig::try_new(tedge_config_location)?,
    );
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
