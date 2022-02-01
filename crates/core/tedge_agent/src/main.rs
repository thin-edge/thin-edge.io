use agent::SmAgentConfig;
use structopt::*;

mod agent;
mod error;
mod operation_logs;
mod restart_operation_handler;
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

    /// Start the agent with clean session off, subscribe to the topics, so that no messages are lost
    #[structopt(short, long)]
    pub init: bool,

    /// Start the agent with clean session on, drop the previous session and subscriptions
    ///
    /// WARNING: All pending messages will be lost.
    #[structopt(short, long)]
    pub clear: bool,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let agent_opt = AgentOpt::from_args();
    let tedge_config_location = tedge_config::TEdgeConfigLocation::from_default_system_location();
    let mut agent = agent::SmAgent::try_new(
        "tedge_agent",
        SmAgentConfig::try_new(tedge_config_location)?,
    )?;
    tedge_utils::logging::initialise_tracing_subscriber(agent_opt.debug);
    if agent_opt.init {
        agent.init_session().await?;
    } else if agent_opt.clear {
        agent.clear_session().await?;
    } else {
        agent.start().await?;
    }
    Ok(())
}
