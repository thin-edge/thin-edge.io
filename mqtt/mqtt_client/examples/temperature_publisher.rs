use log::debug;
use log::error;
use log::info;
use mqtt_client::Client;
use mqtt_client::Config;
use mqtt_client::Message;
use mqtt_client::Topic;
use rand::prelude::*;
use std::time::Duration;
use tokio::time::sleep;

const C8Y_TPL_RESTART: &str = "510";
const C8Y_TPL_TEMPERATURE: &str = "211";

#[tokio::main]
pub async fn main() -> Result<(), mqtt_client::Error> {
    let mqtt = Client::connect("temperature", &Config::default()).await?;
    let c8y_msg = Topic::new("c8y/s/us")?;
    let c8y_cmd = Topic::new("c8y/s/ds")?;
    let c8y_err = Topic::new("c8y/s/e")?;

    init_logger();

    tokio::select! {
        _ = publish_temperature(&mqtt, c8y_msg) => (),
        _ = listen_command(&mqtt, c8y_cmd) => (),
        _ = listen_c8y_error(&mqtt, c8y_err) => (),
        _ = listen_error(&mqtt) => (),
    }

    mqtt.disconnect().await
}

async fn publish_temperature(mqtt: &Client, c8y_msg: Topic) -> Result<(), mqtt_client::Error> {
    let mut rng = thread_rng();
    let mut temperature: i32 = rng.gen_range(-10, 20);

    info!("Publishing temperature measurements");
    for _ in 1..10 {
        let delta = rng.gen_range(-1, 2);
        temperature = temperature + delta;

        let payload = format!("{},{}", C8Y_TPL_TEMPERATURE, temperature);
        debug!("{}", payload);
        mqtt.publish(Message::new(&c8y_msg, payload)).await?;

        sleep(Duration::from_millis(1000)).await;
    }

    Ok(())
}

async fn listen_command(mqtt: &Client, c8y_cmd: Topic) -> Result<(), mqtt_client::Error> {
    let mut messages = mqtt.subscribe(c8y_cmd.filter()).await?;

    while let Some(message) = messages.next().await {
        debug!("C8Y command: {:?}", message.payload);
        if let Some(cmd) = std::str::from_utf8(&message.payload).ok() {
            if cmd.contains(C8Y_TPL_RESTART) {
                info!("Stopping on remote request ... should be restarted by the daemon monitor.");
                break;
            }
        }
    }

    Ok(())
}

async fn listen_c8y_error(mqtt: &Client, c8y_err: Topic) -> Result<(), mqtt_client::Error> {
    let mut messages = mqtt.subscribe(c8y_err.filter()).await?;

    while let Some(message) = messages.next().await {
        error!("C8Y error: {:?}", message.payload);
    }

    Ok(())
}

async fn listen_error(mqtt: &Client) {
    let mut errors = mqtt.subscribe_errors();

    while let Some(error) = errors.next().await {
        error!("System error: {}", error);
    }
}

fn init_logger() {
    let logger = env_logger::Logger::from_default_env();
    async_log::Logger::wrap(logger, || 12)
        .start(log::LevelFilter::Trace)
        .unwrap();
}
