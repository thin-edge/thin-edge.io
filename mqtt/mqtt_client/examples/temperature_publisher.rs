use futures::future::FutureExt;
use futures::select;
use futures_timer::Delay;
use log::debug;
use log::error;
use log::info;
use mqtt_client::Config;
use mqtt_client::Message;
use mqtt_client::Topic;
use mqtt_client::{Client, ErrorStream, MessageStream};
use rand::prelude::*;
use std::time::Duration;

const C8Y_TEMPLATE_RESTART: &str = "510";
const C8Y_TEMPLATE_TEMPERATURE: &str = "211";

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let c8y_msg = Topic::new("c8y/s/us")?;
    let c8y_cmd = Topic::new("c8y/s/ds")?;
    let c8y_err = Topic::new("c8y/s/e")?;

    init_logger();

    let mqtt = Client::connect("temperature", &Config::default()).await?;

    let commands = mqtt.subscribe(c8y_cmd.filter()).await?;
    let c8y_errors = mqtt.subscribe(c8y_err.filter()).await?;
    let errors = mqtt.subscribe_errors();

    tokio::spawn(publish_temperature(mqtt, c8y_msg));

    select! {
        _ = listen_command(commands).fuse() => (),
        _ = listen_c8y_error(c8y_errors).fuse() => (),
        _ = listen_error(errors).fuse() => (),
    }

    Ok(())
}

async fn publish_temperature(mut mqtt: Client, c8y_msg: Topic) -> Result<(), mqtt_client::Error> {
    let mut temperature: i32 = random_in_range(-10, 20);

    info!("Publishing temperature measurements");
    for _ in 1..10 {
        let delta = random_in_range(-1, 2);
        temperature = temperature + delta;

        let payload = format!("{},{}", C8Y_TEMPLATE_TEMPERATURE, temperature);
        debug!("{}", payload);
        mqtt.publish(Message::new(&c8y_msg, payload)).await?;

        Delay::new(Duration::from_millis(1000)).await;
    }

    mqtt.disconnect().await?;
    Ok(())
}

fn random_in_range(low: i32, high: i32) -> i32 {
    let mut rng = thread_rng();
    rng.gen_range(low, high)
}

async fn listen_command(mut messages: MessageStream) {
    while let Some(message) = messages.next().await {
        debug!("C8Y command: {:?}", message.payload);
        if let Some(cmd) = std::str::from_utf8(&message.payload).ok() {
            if cmd.contains(C8Y_TEMPLATE_RESTART) {
                info!("Stopping on remote request ... should be restarted by the daemon monitor.");
                break;
            }
        }
    }
}

async fn listen_c8y_error(mut messages: MessageStream) {
    while let Some(message) = messages.next().await {
        error!("C8Y error: {:?}", message.payload);
    }
}

async fn listen_error(mut errors: ErrorStream) {
    while let Some(error) = errors.next().await {
        error!("System error: {}", error);
    }
}

fn init_logger() {
    let logger = env_logger::Logger::from_default_env();
    let task_id = 1;

    async_log::Logger::wrap(logger, move || task_id)
        .start(log::LevelFilter::Trace)
        .unwrap();
}
