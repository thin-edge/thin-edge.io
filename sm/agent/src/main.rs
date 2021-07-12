use crate::component::TEdgeComponent;

mod component;
mod error;
mod sm_manager;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    initialise_logging();

    let component = sm_manager::SmManager::new("abc");
    component.start().await
}

fn initialise_logging() {
    tracing_subscriber::fmt()
        .with_timer(tracing_subscriber::fmt::time::ChronoUtc::with_format(
            "%Y-%m-%dT%H:%M:%S%.3f%:z".into(),
        ))
        .init();
}
