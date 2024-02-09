use cap::Cap;
use clap::Parser;
use std::alloc;

#[global_allocator]
static ALLOCATOR: Cap<alloc::System> = Cap::new(alloc::System, usize::MAX);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let agent_opt = tedge_agent::AgentOpt::parse();
    let tedge_config = tedge_config::load_tedge_config(&agent_opt.config_dir)?;
    let log_memory_interval = tedge_config.run.log_memory_interval.duration();
    if !log_memory_interval.is_zero() {
        tokio::spawn(async move {
            loop {
                log::info!("Allocated memory: {} Bytes", ALLOCATOR.allocated());
                tokio::time::sleep(log_memory_interval).await;
            }
        });
    }

    tedge_agent::run(agent_opt).await
}
