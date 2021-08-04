use tedge_users::UserManager;
use tedge_utils::paths::{home_dir, PathsError};

mod agent;
mod component;
mod error;
mod state;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let user_manager = UserManager::new();
    let _user_guard = user_manager.become_user(tedge_users::TEDGE_USER)?;

    let tedge_config_location = if tedge_users::UserManager::running_as_root() {
        tedge_config::TEdgeConfigLocation::from_default_system_location()
    } else {
        tedge_config::TEdgeConfigLocation::from_users_home_location(
            home_dir().ok_or(PathsError::HomeDirNotFound)?,
        )
    };

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
