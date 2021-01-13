use env_logger::Env;

use log;

use mqtt_client::Client;

mod mapper;

const DEFAULT_LOG_LEVEL: &str = "warn";
const NAME: &str = "tedge_mapper";

#[tokio::main]
async fn main() -> Result<(), mqtt_client::Error> {
    env_logger::Builder::from_env(Env::default().default_filter_or(DEFAULT_LOG_LEVEL)).init();

    log::info!("tedge-mapper starting!");

    let config = mqtt_client::Config::default();
    let mqtt = Client::connect(NAME, &config).await?;

    let mapper = mapper::Mapper::new(
        mqtt,
        mapper::IN_TOPIC,
        mapper::C8Y_TOPIC_C8Y_JSON,
        mapper::ERRORS_TOPIC,
    )?;
    mapper.subscribe_messages().await?;

    Ok(())
}
