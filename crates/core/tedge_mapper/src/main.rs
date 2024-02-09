use cap::Cap;
use clap::Parser;
use std::alloc;
use tracing::log;

#[global_allocator]
static ALLOCATOR: Cap<alloc::System> = Cap::new(alloc::System, usize::MAX);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mapper_opt = tedge_mapper::MapperOpt::parse();
    let tedge_config = tedge_config::load_tedge_config(&mapper_opt.config_dir)?;
    let log_memory_interval = tedge_config.run.log_memory_interval.duration();
    if !log_memory_interval.is_zero() {
        tokio::spawn(async move {
            loop {
                log::info!("Allocated memory: {} Bytes", ALLOCATOR.allocated());
                tokio::time::sleep(log_memory_interval).await;
            }
        });
    }

    tedge_mapper::run(mapper_opt).await
}
