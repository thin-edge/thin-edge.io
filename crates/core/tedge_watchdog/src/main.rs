use clap::Parser;
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use mqtt_channel::{Config, Message, MqttError, PubChannel, Topic};
use nanoid::nanoid;
use std::{
    path::PathBuf,
    process::{self, Command, Stdio},
};

use freedesktop_entry_parser::parse_entry;
use serde::{Deserialize, Serialize};
use tedge_config::{
    ConfigRepository, ConfigSettingAccessor, ConfigSettingError, MqttBindAddressSetting,
    MqttPortSetting, TEdgeConfigError, TEdgeConfigLocation, DEFAULT_TEDGE_CONFIG_PATH
};
use tokio::time::{error::Elapsed, Duration};

#[derive(Serialize, Deserialize)]
pub struct HealthStatus {
    status: String,
    pid: u32,
}

#[derive(Debug, clap::Parser)]
#[clap(
name = clap::crate_name!(),
version = clap::crate_version!(),
about = clap::crate_description!()
)]
pub struct WatchdogOpt {
    /// Turn-on the debug log level.
    ///
    /// If off only reports ERROR, WARN, and INFO
    /// If on also reports DEBUG and TRACE
    #[clap(long)]
    pub debug: bool,

    /// Start the watchdog from custom path
    ///
    /// WARNING: This is mostly used in testing.
    #[clap(long = "config-dir", default_value = DEFAULT_TEDGE_CONFIG_PATH)]
    pub config_dir: PathBuf,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let watchdog_opt = WatchdogOpt::parse();
    tedge_utils::logging::initialise_tracing_subscriber(watchdog_opt.debug);

    start_watchdog(watchdog_opt.config_dir).await?;
    Ok(())
}

async fn start_watchdog(tedge_config_dir: PathBuf) -> Result<(), anyhow::Error> {
    // Send ready notification to systemd.
    let _ = notify_systemd(process::id(), "--ready");

    let tedge_services = vec![
        "tedge-mapper-c8y",
        "tedge-mapper-az",
        "tedge-mapper-collectd",
        "tedge-agent",
    ];
    let watchdog_threads = FuturesUnordered::new();

    for service in tedge_services {
        let interval = match get_watchdog_sec(&format!("/lib/systemd/system/{}.service", service)) {
            Ok(i) => i,
            Err(_e) => continue, // Watchdog not enabled for this service
        };
        let req_topic = format!("tedge/health-check/{}", service);
        let res_topic = format!("tedge/health/{}", service);
        let tedge_config_location =
            tedge_config::TEdgeConfigLocation::from_custom_root(tedge_config_dir.clone());
        if interval > 0 {
            watchdog_threads.push(tokio::spawn(async move {
                monitor_tedge_service(
                    tedge_config_location,
                    service,
                    &req_topic,
                    &res_topic,
                    interval / 2,
                )
                .await
            }));
        }
    }

    futures::future::join_all(watchdog_threads).await;

    Ok(())
}

async fn monitor_tedge_service(
    tedge_config_location: TEdgeConfigLocation,
    name: &str,
    req_topic: &str,
    res_topic: &str,
    interval: u64,
) -> Result<(), WatchdogError> {
    let client_id: &str = &format!("{}_{}", name, nanoid!());
    let mqtt_config = get_mqtt_config(tedge_config_location, client_id)?
        .with_subscriptions(res_topic.try_into()?);
    let client = mqtt_channel::Connection::new(&mqtt_config).await?;
    let mut received = client.received;
    let mut publisher = client.published;

    println!("Starting watchdog for {} service", name);

    loop {
        let message = Message::new(&Topic::new(req_topic)?, "");
        match publisher.publish(message).await {
            Ok(()) => {}
            Err(e) => {
                eprintln!("Publish failed with error: {}", e.to_string());
            }
        }

        let freq = interval + 1;

        match tokio::time::timeout(tokio::time::Duration::from_secs(freq), received.next()).await {
            Ok(Some(msg)) => {
                let message = msg.payload_str()?;

                let p: HealthStatus = serde_json::from_str(message)?;

                notify_systemd(p.pid, "WATCHDOG=1")?;
            }
            Ok(None) => {}
            Err(elapsed) => {
                let err = WatchdogError::HealthStatusTimeElapsed {
                    service: name.to_string(),
                    from: elapsed,
                };
                eprintln!("{}", err);
            }
        }
        tokio::time::sleep(Duration::from_secs(interval)).await;
    }
}

fn get_mqtt_config(
    tedge_config_location: TEdgeConfigLocation,
    client_id: &str,
) -> Result<Config, WatchdogError> {
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

fn get_watchdog_sec(service_file: &str) -> Result<u64, WatchdogError> {
    let entry = parse_entry(service_file)?;
    if let Some(interval) = entry.section("Service").attr("WatchdogSec") {
        Ok(interval.parse()?)
    } else {
        Ok(0)
    }
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

    #[error(transparent)]
    DeserializeError(#[from] serde_json::Error),

    #[error("Health status elapsed for service `{service}`: {from}")]
    HealthStatusTimeElapsed { service: String, from: Elapsed },

    #[error("Failed to parse watchdogsec")]
    ParseWatchdogSec(#[from] std::num::ParseIntError),

    #[error(transparent)]
    ParseSystemdFile(#[from] std::io::Error),
}
