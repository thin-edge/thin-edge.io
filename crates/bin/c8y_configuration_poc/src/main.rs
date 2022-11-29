mod config_ext;
mod file_system_ext;
mod http_ext;
mod mqtt_ext;

use tedge_actors::Runtime;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let runtime_events_logger = None;
    let runtime = Runtime::try_new(runtime_events_logger).await?;




    runtime.run_to_completion().await?;
    Ok(())



}
