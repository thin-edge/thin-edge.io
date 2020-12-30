use env_logger::Env;

// use rumqttc::AsyncClient;
// use rumqttc::Event;
// use rumqttc::Incoming;
// use rumqttc::MqttOptions;
// use rumqttc::QoS;

use std::error::Error;

mod mapper;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::Builder::from_env(Env::default().default_filter_or("warn")).init();

    log::info!("tedge-mapper starting!");

    let mut mapper = mapper::Mapper::new();
    mapper.connect().await?;
    mapper.run_forever().await?;

    Ok(())
}
