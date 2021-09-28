use agent::SmAgentConfig;
use structopt::*;

mod agent;
mod error;
mod operation_logs;
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
    tedge_utils::logging::initialise_tracing_subscriber(agent.debug);
    let tedge_config_location = tedge_config::TEdgeConfigLocation::from_default_system_location();
    let agent = agent::SmAgent::try_new(
        "tedge_agent",
        SmAgentConfig::try_new(tedge_config_location)?,
    )?;
    agent.start().await?;
    Ok(())
}
