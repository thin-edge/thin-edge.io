use inotify::{EventMask, Inotify, WatchMask};
use rumqttc::{Event, MqttOptions, Outgoing, Packet, QoS::AtLeastOnce};
use serde::{Deserialize, Serialize};
use tracing::info;

const DISCOVER_OPS_CLIENT_ID: &str = "DISCOVER_OPERATIONS";

#[derive(Serialize, Deserialize, Debug)]
pub enum EventType {
    ADD,
    REMOVE,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DiscoverOp {
    pub ops_dir: String,
    pub event_type: EventType,
    pub operation_name: String,
}

#[derive(thiserror::Error, Debug)]
#[allow(clippy::enum_variant_names)]
pub enum DynamicDiscoverOpsError {
    #[error(transparent)]
    ClientError(#[from] rumqttc::ClientError),

    #[error(transparent)]
    SerializeError(#[from] serde_json::Error),

    #[error("Event name is empty")]
    NoEventName,
}

pub async fn discover_operations(
    ops_dir: String,
    mqtt_host: &str,
    mqtt_port: &u16,
) -> Result<(), DynamicDiscoverOpsError> {
    info!("Start the operation discovery service");
    let mut inotify = Inotify::init().expect("Error while initializing inotify instance");

    // Watch for modify and close events.
    inotify
        .add_watch(ops_dir.clone(), WatchMask::CREATE | WatchMask::DELETE)
        .expect("Failed to add file watch");

    // Read events that were added with `add_watch` above.
    let mut buffer = [0; 1024];

    loop {
        let events = inotify
            .read_events_blocking(&mut buffer)
            .expect("Error while reading events");

        for event in events {
            info!("Event : {:?}", event);
            let fname = event
                .name
                .ok_or(DynamicDiscoverOpsError::NoEventName)?
                .to_str()
                .ok_or(DynamicDiscoverOpsError::NoEventName)?;
            match event.mask {
                EventMask::CREATE => {
                    publish(
                        &ops_dir,
                        EventType::ADD,
                        fname.to_string(),
                        &mqtt_host,
                        mqtt_port,
                    )
                    .await?;
                }
                EventMask::DELETE => {
                    publish(
                        &ops_dir,
                        EventType::REMOVE,
                        fname.to_string(),
                        &mqtt_host,
                        mqtt_port,
                    )
                    .await?;
                }
                _ => {}
            }
        }
    }
}

async fn publish(
    ops_dir: &str,
    event_type: EventType,
    operation_name: String,
    mqtt_host: &str,
    mqtt_port: &u16,
) -> Result<(), DynamicDiscoverOpsError> {
    let mut options = MqttOptions::new(DISCOVER_OPS_CLIENT_ID, mqtt_host, *mqtt_port);
    options.set_clean_session(true);

    let payload = serde_json::to_string(&DiscoverOp {
        ops_dir: ops_dir.into(),
        event_type,
        operation_name,
    })?;

    let (client, mut connection) = rumqttc::AsyncClient::new(options, 10);
    let mut published = false;

    client
        .publish(
            "tedge/operation/update",
            AtLeastOnce,
            false,
            payload.clone(),
        )
        .await?;

    loop {
        match connection.poll().await {
            Ok(Event::Outgoing(Outgoing::Publish(_))) => {}
            Ok(Event::Incoming(Packet::PubAck(_))) => {
                println!("publish event");
                published = true;
                break;
            }
            _ => {}
        }
    }

    if !published {
        eprintln!("ERROR: the message has not been published");
    }

    client.disconnect().await?;
    Ok(())
}
