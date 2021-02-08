use env_logger::Env;

use client::Client;

mod mapper;

const DEFAULT_LOG_LEVEL: &str = "warn";
const APP_NAME: &str = "tedge-mapper";

#[tokio::main]
async fn main() -> Result<(), client::Error> {
    env_logger::Builder::from_env(Env::default().default_filter_or(DEFAULT_LOG_LEVEL)).init();

    log::info!("tedge-mapper starting!");

    let config = client::Config::default();
    let mqtt = Client::connect(APP_NAME, &config).await?;

    let mapper = mapper::Mapper::new_from_string(
        mqtt,
        mapper::IN_TOPIC,
        mapper::C8Y_TOPIC_C8Y_JSON,
        mapper::ERRORS_TOPIC,
    )?;
    mapper.run().await?;

    Ok(())
}
