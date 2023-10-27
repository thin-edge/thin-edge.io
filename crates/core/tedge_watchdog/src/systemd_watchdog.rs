use crate::error::WatchdogError;
use anyhow::Context;
use freedesktop_entry_parser::parse_entry;
use futures::channel::mpsc;
use futures::stream::FuturesUnordered;
use futures::SinkExt;
use futures::StreamExt;
use mqtt_channel::Message;
use mqtt_channel::PubChannel;
use mqtt_channel::Topic;
use serde::Deserialize;
use serde::Serialize;
use std::path::PathBuf;
use std::process;
use std::process::Command;
use std::process::ExitStatus;
use std::process::Stdio;
use std::time::Instant;
use tedge_api::health::ServiceHealthTopic;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::OperationType;
use tedge_config::TEdgeConfigLocation;
use time::format_description;
use time::OffsetDateTime;
use tracing::debug;
use tracing::error;
use tracing::info;
use tracing::warn;

const SERVICE_NAME: &str = "tedge-watchdog";

// TODO: extract to common module
#[derive(Debug, Serialize, Deserialize)]
pub struct HealthStatus {
    status: String,
    pid: u32,
    time: String,
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
    // let tedge_config_location = tedge_config::TEdgeConfigLocation::from_custom_root(&config_dir);
    let tedge_config_location =
        tedge_config::TEdgeConfigLocation::from_custom_root(tedge_config_dir.clone());
    let config_repository = tedge_config::TEdgeConfigRepository::new(tedge_config_location.clone());
    let tedge_config = config_repository.load().expect("Could not load config");

    let mqtt_topic_root = tedge_config.mqtt.topic_root.clone();
    let mqtt_schema = MqttSchema::with_root(mqtt_topic_root);

    // TODO: now that we have entity registration, instead of hardcoding, the watchdog can see all
    // running services by looking at registration messages
    let device_topic_id = tedge_config
        .mqtt
        .device_topic_id
        .parse::<EntityTopicId>()
        .expect("Services not in default scheme unsupported");

    // let device_topic_id = tedge_config_dir
    let tedge_services = vec![
        "tedge-mapper-c8y",
        "tedge-mapper-az",
        "tedge-mapper-aws",
        "tedge-mapper-collectd",
        "tedge-agent",
        "tedge-log-plugin",
        "tedge-configuration-plugin",
        "c8y-firmware-plugin",
    ]
    .into_iter()
    .map(|s| {
        device_topic_id
            .default_service_for_device(s)
            .expect("Services not in default scheme unsupported")
    })
    .collect::<Vec<_>>();

    let watchdog_tasks = FuturesUnordered::new();

    for service in tedge_services {
        let service_name = service.default_service_name().unwrap();
        match get_watchdog_sec(&format!("/lib/systemd/system/{service_name}.service")) {
            Ok(interval) => {
                let req_topic = mqtt_schema.topic_for(
                    &service,
                    &Channel::Command {
                        operation: OperationType::Health,
                        cmd_id: "check".to_string(),
                    },
                );
                let res_topic = mqtt_schema.topic_for(&service, &Channel::Health);

                let tedge_config_location = tedge_config_location.clone();
                watchdog_tasks.push(tokio::spawn(async move {
                    monitor_tedge_service(
                        tedge_config_location,
                        service.as_str(),
                        req_topic,
                        res_topic,
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
    req_topic: Topic,
    res_topic: Topic,
    interval: u64,
) -> Result<(), WatchdogError> {
    let config_repository = tedge_config::TEdgeConfigRepository::new(tedge_config_location);
    let tedge_config = config_repository.load()?;

    let mqtt_device_topic_id: EntityTopicId = tedge_config
        .mqtt
        .device_topic_id
        .parse()
        .context("Can't parse as device topic id")?;

    let mqtt_topic_root = &tedge_config.mqtt.topic_root;

    let mqtt_session_name = format!("{SERVICE_NAME}#{mqtt_topic_root}/{mqtt_device_topic_id}");

    let mqtt_schema = MqttSchema::with_root(mqtt_topic_root.clone());

    let service_topic_id = mqtt_device_topic_id
        .default_service_for_device(SERVICE_NAME)
        .unwrap();
    let service_health_topic =
        ServiceHealthTopic::from_new_topic(&service_topic_id.into(), &mqtt_schema);

    let _service_health_topic = service_health_topic.clone();

    let mqtt_config = tedge_config
        .mqtt_config()?
        .with_session_name(mqtt_session_name)
        .with_subscriptions(res_topic.into())
        .with_initial_message(move || {
            _service_health_topic
                .clone()
                .up_message(&mqtt_schema.clone())
        })
        .with_last_will_message(service_health_topic.down_message());

    let client = mqtt_channel::Connection::new(&mqtt_config).await?;

    let mut received = client.received;
    let mut publisher = client.published;

    info!("Starting watchdog for {} service", name);

    // Now the systemd watchdog is done with the initialization and ready for processing the messages
    let health_status_message =
        service_health_topic.up_message(&MqttSchema::with_root(mqtt_topic_root.clone()));
    publisher
        .send(health_status_message)
        .await
        .context("Could not send initial health status message")?;

    loop {
        let message = Message::new(&req_topic, "");
        let _ = publisher
            .publish(message)
            .await
            .map_err(|e| warn!("Publish failed with error: {}", e));

        let start = Instant::now();

        let request_timestamp = OffsetDateTime::now_utc();
        let request_timestamp = request_timestamp
            .format(&time::format_description::well_known::Rfc3339)?
            .as_str()
            .into();
        match tokio::time::timeout(
            tokio::time::Duration::from_secs(interval),
            get_latest_health_status_message(request_timestamp, &mut received),
        )
        .await
        {
            Ok(health_status) => {
                let health_status = health_status?;
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
    request_timestamp: String,
    messages: &mut mpsc::UnboundedReceiver<Message>,
) -> Result<HealthStatus, WatchdogError> {
    loop {
        if let Some(message) = messages.next().await {
            if let Ok(message) = message.payload_str() {
                debug!("Health response received: {}", message);
                if let Ok(health_status) = serde_json::from_str::<HealthStatus>(message) {
                    let request_timestamp = OffsetDateTime::parse(
                        &request_timestamp,
                        &format_description::well_known::Rfc3339,
                    )?
                    .unix_timestamp();
                    let datetime = OffsetDateTime::parse(
                        &health_status.time,
                        &format_description::well_known::Rfc3339,
                    )?
                    .unix_timestamp();

                    if datetime >= request_timestamp {
                        return Ok(health_status);
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
    use std::time::Duration;

    use anyhow::Result;
    use serde_json::json;

    use super::*;

    #[tokio::test]
    async fn test_get_latest_health_status_message() -> Result<()> {
        let (mut sender, mut receiver) = mpsc::unbounded::<Message>();
        let health_topic =
            Topic::new("te/device/main/service/test-service/status/health").expect("Valid topic");
        let base_timestamp = OffsetDateTime::now_utc();

        for x in 1..5u64 {
            let incremented_datetime = base_timestamp + Duration::from_secs(x);
            let timestamp_str = incremented_datetime
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap();

            let health_status = json!({
                "status": "up",
                "pid": 123u32,
                "time": timestamp_str,
            })
            .to_string();
            let health_message = Message::new(&health_topic, health_status);
            sender.publish(health_message).await?;
        }

        let base_timestamp_str = (base_timestamp + Duration::from_secs(3))
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap();
        let health_status =
            get_latest_health_status_message(base_timestamp_str.clone(), &mut receiver).await;

        assert_eq!(health_status.unwrap().time, base_timestamp_str);

        let base_timestamp_str = (base_timestamp + Duration::from_secs(5))
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap();

        let timeout_error = tokio::time::timeout(
            tokio::time::Duration::from_secs(1),
            get_latest_health_status_message(base_timestamp_str, &mut receiver),
        )
        .await;
        assert!(timeout_error.is_err());

        Ok(())
    }
}
