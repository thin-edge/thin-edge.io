use env_logger::Env;
use mapper::{Mapper, MapperError};
use service::*;

mod mapper;
mod service;

const DEFAULT_LOG_LEVEL: &str = "warn";

#[tokio::main]
async fn main() -> Result<(), ServiceError<MapperError>> {
    env_logger::Builder::from_env(Env::default().default_filter_or(DEFAULT_LOG_LEVEL)).init();

    ServiceRunner::<Mapper>::new()
        .run_with_default_config()
        .await
}
