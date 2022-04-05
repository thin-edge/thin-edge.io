use crate::error::WatchdogError;
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use mqtt_channel::{Config, Message, PubChannel, Topic};
use nanoid::nanoid;

use freedesktop_entry_parser::parse_entry;
use serde::{Deserialize, Serialize};
use std::time::Instant;
use std::{
    path::PathBuf,
    process::{self, Command, ExitStatus, Stdio},
};
use tedge_config::{
    ConfigRepository, ConfigSettingAccessor, MqttBindAddressSetting, MqttPortSetting,
    TEdgeConfigLocation,
};

#[derive(Serialize, Deserialize)]
pub struct HealthStatus {
    status: String,
    pid: u32,
}

pub async fn start_watchdog(tedge_config_dir: PathBuf) -> Result<(), anyhow::Error> {
    // Send ready notification to systemd.
    notify_systemd(process::id(), "--ready")?;

    let tedge_services = vec![
        "tedge-mapper-c8y",
        "tedge-mapper-az",
        "tedge-mapper-collectd",
        "tedge-agent",
    ];

    let watchdog_threads = FuturesUnordered::new();

    for service in tedge_services {
        let interval = match get_watchdog_sec(&format!("/lib/systemd/system/{service}.service")) {
            Ok(i) => i,
            Err(_e) => continue, // Watchdog not enabled for this service
        };
        let req_topic = format!("tedge/health-check/{service}");
        let res_topic = format!("tedge/health/{service}");
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
        let _ = publisher
            .publish(message)
            .await
            .map_err(|e| eprintln!("Publish failed with error: {}", e));

        let start = Instant::now();

        match tokio::time::timeout(tokio::time::Duration::from_secs(interval), received.next())
            .await
        {
            Ok(Some(msg)) => {
                let message = msg.payload_str()?;

                let p: HealthStatus = serde_json::from_str(message)?;

                notify_systemd(p.pid, "WATCHDOG=1")?;
            }
            Ok(None) => {}
            Err(elapsed) => {
                eprintln!("The {name} failed with {elapsed}");
            }
        }
        tokio::time::sleep(start.elapsed()).await;
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

fn notify_systemd(pid: u32, status: &str) -> Result<ExitStatus, WatchdogError> {
    let pid_opt = format!("--pid={pid}");
    Ok(Command::new("systemd-notify")
        .args([status, &pid_opt])
        .stdin(Stdio::null())
        .status()
        .map_err(|err| WatchdogError::CommandExecError {
            cmd: String::from("systemd-notify"),
            from: err,
        })?)
}

fn get_watchdog_sec(service_file: &str) -> Result<u64, WatchdogError> {
    let entry = parse_entry(service_file)?;
    if let Some(interval) = entry.section("Service").attr("WatchdogSec") {
        Ok(interval.parse()?)
    } else {
        Err(WatchdogError::NoWatchdogSec)
    }
}
