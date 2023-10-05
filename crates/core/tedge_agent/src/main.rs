use clap::Parser;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let agent_opt = tedge_agent::AgentOpt::parse();
    tedge_agent::run(agent_opt).await
}
