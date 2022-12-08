use crate::error::WatchdogError;
use freedesktop_entry_parser::parse_entry;
use futures::channel::mpsc;
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use mqtt_channel::Config;
use mqtt_channel::Message;
use mqtt_channel::PubChannel;
use mqtt_channel::Topic;
use nanoid::nanoid;
use serde::Deserialize;
use serde::Serialize;
use std::path::PathBuf;
use std::process::Command;
use std::process::ExitStatus;
use std::process::Stdio;
use std::process::{self};
use std::time::Instant;
use tedge_api::health::get_health_status_down_message;
use tedge_api::health::send_health_status;
use tedge_config::ConfigRepository;
use tedge_config::ConfigSettingAccessor;
use tedge_config::MqttBindAddressSetting;
use tedge_config::MqttPortSetting;
use tedge_config::TEdgeConfigLocation;
use time::OffsetDateTime;
use tracing::debug;
use tracing::error;
use tracing::info;
use tracing::warn;

#[derive(Debug, Serialize, Deserialize)]
pub struct HealthStatus {
    status: String,
    pid: u32,
    time: i64,
}

pub async fn start_watchdog(tedge_config_dir: PathBuf) -> Result<(), anyhow::Error> {
    // Send ready notification to systemd.
    notify_systemd(process::id(), "--ready")?;

    // Send heart beat notifications to systemd, to notify about its own health status
    start_watchdog_for_self().await?;

    // Monitor health of tedge services
    start_watchdog_for_tedge_services(tedge_config_dir).await;
    Ok(())
}

async fn start_watchdog_for_self() -> Result<(), WatchdogError> {
    match get_watchdog_sec("/lib/systemd/system/tedge-watchdog.service") {
        Ok(interval) => {
            let _handle = tokio::spawn(async move {
                loop {
                    let _ = notify_systemd(process::id(), "WATCHDOG=1").map_err(|e| {
                        eprintln!("Notifying systemd failed with {}", e);
                    });
                    tokio::time::sleep(tokio::time::Duration::from_secs(interval / 4)).await;
                }
            });
            Ok(())
        }

        Err(WatchdogError::NoWatchdogSec { file }) => {
            warn!(
                "Watchdog is not enabled for tedge-watchdog : {}",
                WatchdogError::NoWatchdogSec { file }
            );
            Ok(())
        }
        Err(e) => Err(e),
    }
}

async fn start_watchdog_for_tedge_services(tedge_config_dir: PathBuf) {
    let tedge_services = vec![
        "tedge-mapper-c8y",
        "tedge-mapper-az",
        "tedge-mapper-collectd",
        "tedge-agent",
        "c8y-log-plugin",
        "c8y-configuration-plugin",
    ];

    let watchdog_tasks = FuturesUnordered::new();

    for service in tedge_services {
        match get_watchdog_sec(&format!("/lib/systemd/system/{service}.service")) {
            Ok(interval) => {
                let req_topic = format!("tedge/health-check/{service}");
                let res_topic = format!("tedge/health/{service}");
                let tedge_config_location =
                    tedge_config::TEdgeConfigLocation::from_custom_root(tedge_config_dir.clone());

                watchdog_tasks.push(tokio::spawn(async move {
                    monitor_tedge_service(
                        tedge_config_location,
                        service,
                        &req_topic,
                        &res_topic,
                        interval / 4,
                    )
                    .await
                }));
            }

            Err(_) => {
                warn!("Watchdog is not enabled for {}", service);
                continue;
            }
        }
    }
    futures::future::join_all(watchdog_tasks).await;
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
        .with_subscriptions(res_topic.try_into()?)
        .with_last_will_message(get_health_status_down_message("tedge-watchdog"));
    let client = mqtt_channel::Connection::new(&mqtt_config).await?;
    let mut received = client.received;
    let mut publisher = client.published;

    info!("Starting watchdog for {} service", name);

    // Now the systemd watchdog is done with the initialization and ready for processing the messages
    send_health_status(&mut publisher, "tedge-watchdog").await;

    loop {
        let message = Message::new(&Topic::new(req_topic)?, "");
        let _ = publisher
            .publish(message)
            .await
            .map_err(|e| warn!("Publish failed with error: {}", e));

        let start = Instant::now();

        let request_timestamp = OffsetDateTime::now_utc().unix_timestamp();
        match tokio::time::timeout(
            tokio::time::Duration::from_secs(interval),
            get_latest_health_status_message(request_timestamp, &mut received),
        )
        .await
        {
            Ok(health_status) => {
                debug!(
                    "Sending notification for {} with pid: {}",
                    name, health_status.pid
                );
                notify_systemd(health_status.pid, "WATCHDOG=1")?;
            }
            Err(_) => {
                warn!("No health check response received from {name} in time");
            }
        }

        let elapsed = start.elapsed();
        if elapsed < tokio::time::Duration::from_secs(interval) {
            tokio::time::sleep(tokio::time::Duration::from_secs(interval) - elapsed).await;
            warn!("tedge systemd watchdog not started because no services to monitor")
        }
    }
}

async fn get_latest_health_status_message(
    request_timestamp: i64,
    messages: &mut mpsc::UnboundedReceiver<Message>,
) -> HealthStatus {
    loop {
        if let Some(message) = messages.next().await {
            if let Ok(message) = message.payload_str() {
                debug!("Health response received: {}", message);
                if let Ok(health_status) = serde_json::from_str::<HealthStatus>(message) {
                    if health_status.time >= request_timestamp {
                        return health_status;
                    } else {
                        debug!(
                            "Ignoring stale health response: {:?} older than request time: {}",
                            health_status, request_timestamp
                        );
                    }
                } else {
                    error!("Invalid health response received: {}", message);
                }
            }
        }
    }
}

fn get_mqtt_config(
    tedge_config_location: TEdgeConfigLocation,
    client_id: &str,
) -> Result<Config, WatchdogError> {
    let config_repository = tedge_config::TEdgeConfigRepository::new(tedge_config_location);
    let tedge_config = config_repository.load()?;
    let mqtt_config = Config::default()
        .with_session_name(client_id)
        .with_host(tedge_config.query(MqttBindAddressSetting)?.to_string())
        .with_port(tedge_config.query(MqttPortSetting)?.into());
    Ok(mqtt_config)
}

fn notify_systemd(pid: u32, status: &str) -> Result<ExitStatus, WatchdogError> {
    let pid_opt = format!("--pid={pid}");
    Command::new("systemd-notify")
        .args([status, &pid_opt])
        .stdin(Stdio::null())
        .status()
        .map_err(|err| WatchdogError::CommandExecError {
            cmd: String::from("systemd-notify"),
            from: err,
        })
}

fn get_watchdog_sec(service_file: &str) -> Result<u64, WatchdogError> {
    let entry = parse_entry(service_file)?;
    if let Some(interval) = entry.section("Service").attr("WatchdogSec") {
        match interval.parse::<u64>() {
            Ok(i) => Ok(i),
            Err(e) => {
                error!(
                    "Failed to parse the to WatchdogSec to integer from {}",
                    service_file
                );
                Err(WatchdogError::ParseWatchdogSecToInt(e))
            }
        }
    } else {
        Err(WatchdogError::NoWatchdogSec {
            file: service_file.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use serde_json::json;

    use super::*;

    #[tokio::test]
    async fn test_get_latest_health_status_message() -> Result<()> {
        let (mut sender, mut receiver) = mpsc::unbounded::<Message>();
        let health_topic = Topic::new("tedge/health/test-service").expect("Valid topic");

        for x in 1..5i64 {
            let health_status = json!({
                "status": "up",
                "pid": 123u32,
                "time": x,
            })
            .to_string();
            let health_message = Message::new(&health_topic, health_status);
            sender.publish(health_message).await?;
        }

        let health_status = get_latest_health_status_message(3, &mut receiver).await;
        assert_eq!(health_status.time, 3);

        let timeout_error = tokio::time::timeout(
            tokio::time::Duration::from_secs(1),
            get_latest_health_status_message(5, &mut receiver),
        )
        .await;
        assert!(timeout_error.is_err());

        Ok(())
    }
}
