use agent::SmAgentConfig;
use structopt::*;

mod agent;
mod error;
mod state;

#[derive(Debug, StructOpt)]
#[structopt(
name = clap::crate_name!(),
version = clap::crate_version!(),
about = clap::crate_description!()
)]
pub struct AgentOpt {
    /// Turn-on the debug log level.
    ///
    /// If off only reports ERROR, WARN, and INFO
    /// If on also reports DEBUG and TRACE
    #[structopt(long)]
    pub debug: bool,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let agent = AgentOpt::from_args();
    initialise_logging(agent.debug);
    let tedge_config_location = tedge_config::TEdgeConfigLocation::from_default_system_location();
    let agent = agent::SmAgent::new(
        "tedge_agent",
        SmAgentConfig::try_new(tedge_config_location)?,
    );
    agent.start().await?;
    Ok(())
}

fn initialise_logging(debug: bool) {
    let log_level = if debug {
        tracing::Level::TRACE
    } else {
        tracing::Level::INFO
    };

    tracing_subscriber::fmt()
        .with_timer(tracing_subscriber::fmt::time::ChronoUtc::with_format(
            "%Y-%m-%dT%H:%M:%S%.3f%:z".into(),
        ))
        .with_max_level(log_level)
        .init();
}
