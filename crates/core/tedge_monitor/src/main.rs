use futures::stream::FuturesUnordered;
use futures::StreamExt;
use mqtt_channel::{Config, Message, MqttError, PubChannel, Topic};
use nanoid::nanoid;
use std::process::{self, Command, Stdio};

use serde::{Deserialize, Serialize};
use tedge_config::{
    ConfigRepository, ConfigSettingAccessor, ConfigSettingError, MqttBindAddressSetting,
    MqttPortSetting, TEdgeConfigError,
};
use tokio::time::Duration;

pub const DEFAULT_TEDGE_CONFIG_PATH: &str = "/etc/tedge";

#[derive(Serialize, Deserialize)]
pub struct Response {
    status: String,
    pid: u32,
}
#[tokio::main]
async fn main() {
    let _ = start_watchdog().await;
}

async fn start_watchdog() {
    // Send ready notification to systemd.
    let _ = notify_systemd(process::id(), "--ready");

    // Start helth check request publisher
    tokio::spawn(async move { publish().await });

    loop {
        let tedge_services = vec![
            "tedge-mapper-c8y",
            "tedge-mapper-az",
            "tedge-mapper-collectd",
            "tedge-agent",
        ];
        let watchdog_threads = FuturesUnordered::new();

        for service in tedge_services {
            let res_topic = format!("tedge/health/{}", service);

            watchdog_threads.push(tokio::spawn(async move {
                monitor_tedge_service(service, &res_topic).await.unwrap()
            }));
        }

        futures::future::join_all(watchdog_threads).await;
    }
}

async fn publish() -> Result<(), WatchdogError> {
    let client_id: &str = "watchdog_publisher";
    let mqtt_config = get_mqtt_config(client_id)?;

    let client = mqtt_channel::Connection::new(&mqtt_config).await?;

    let mut publisher = client.published;
    let topic = Topic::new("tedge/health-check")?;

    loop {
        let message = Message::new(&topic, &b"\0"[..]);
        match publisher.publish(message).await {
            Ok(()) => {}
            Err(e) => {
                eprintln!("Publish failed with error: {}", e.to_string());
            }
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn monitor_tedge_service(name: &str, res_topic: &str) -> Result<(), WatchdogError> {
    let client_id: &str = &format!("{}_{}", name, nanoid!());
    let mqtt_config = get_mqtt_config(client_id)?.with_subscriptions(res_topic.try_into()?);
    let client = mqtt_channel::Connection::new(&mqtt_config).await?;

    let mut received = client.received;

    println!("started watchdog for service: {}", name);

    while let Some(msg) = received.next().await {
        let message = match msg.payload_str() {
            Ok(m) => m,
            Err(e) => {
                eprintln!("Failed to translate bytes to str: {}", e.to_string());
                continue;
            }
        };

        let p: Response = match serde_json::from_str(message) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("Failed to deserialize str: {}", e.to_string());
                continue;
            }
        };

        match notify_systemd(p.pid, "WATCHDOG=1") {
            Ok(()) => {}
            Err(e) => {
                eprintln!("{}", e.to_string())
            }
        }
    }

    Ok(())
}

fn get_mqtt_config(client_id: &str) -> Result<Config, WatchdogError> {
    let tedge_config_location =
        tedge_config::TEdgeConfigLocation::from_custom_root(DEFAULT_TEDGE_CONFIG_PATH);
    let config_repository = tedge_config::TEdgeConfigRepository::new(tedge_config_location.clone());
    let tedge_config = config_repository.load()?;
    let mqtt_config = Config::default()
        .with_session_name(client_id)
        .with_host(tedge_config.query(MqttBindAddressSetting)?.to_string())
        .with_port(tedge_config.query(MqttPortSetting)?.into());
    Ok(mqtt_config)
}

fn notify_systemd(pid: u32, status: &str) -> Result<(), WatchdogError> {
    let pid_opt = format!("--pid={}", pid);
    let _status = Command::new("systemd-notify")
        .args([status, &pid_opt])
        .stdin(Stdio::null())
        .status()
        .map_err(|err| WatchdogError::CommandExecError {
            cmd: String::from("systemd-notify"),
            from: err,
        })?;
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum WatchdogError {
    #[error("Fail to run `{cmd}`: {from}")]
    CommandExecError { cmd: String, from: std::io::Error },

    #[error(transparent)]
    FromTedgeConfigError(#[from] TEdgeConfigError),

    #[error(transparent)]
    FromConfigSettingError(#[from] ConfigSettingError),

    #[error(transparent)]
    FromMqttError(#[from] MqttError),
}
