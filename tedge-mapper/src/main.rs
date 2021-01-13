use env_logger::Env;

use log;

use mqtt_client::Client;

use std::net::TcpStream;

mod mapper;

const DEFAULT_LOG_LEVEL: &str = "warn";
const EX_NOHOST: i32 = 68;
const NAME: &str = "tedge_mapper";
const MQTT_PORT: u16 = 1883;
const MQTT_URL: &str = "localhost";

#[tokio::main]
async fn main() -> Result<(), mqtt_client::Error> {
    env_logger::Builder::from_env(Env::default().default_filter_or(DEFAULT_LOG_LEVEL)).init();

    log::info!("tedge-mapper starting!");

    let server_available = scan_port(MQTT_PORT);
    if !server_available {
        log::error!("tedge-mapper failed to start, no server available!");
        std::process::exit(EX_NOHOST)
    }

    let config = mqtt_client::Config {
        host: MQTT_URL.to_owned(),
        port: MQTT_PORT,
    };
    let mqtt = Client::connect(NAME, &config).await?;

    // let mapper = mapper::Mapper::new(mqtt, "tedge/measurements", "c8y/s/us", "tedge/errors");
    let mapper = mapper::Mapper::new(
        mqtt,
        "tedge/measurements",
        "c8y/measurement/measurements/create",
        "tedge/errors",
    );
    mapper.subscribe_messages().await?;

    Ok(())
}

/// A simple function to check if port we are trying to connect to is available.
/// This should be part of the client to try to connect and report on connection if an error ocurred.
fn scan_port(port: u16) -> bool {
    match TcpStream::connect(("localhost", port)) {
        Ok(_) => true,
        Err(_) => false,
    }
}
