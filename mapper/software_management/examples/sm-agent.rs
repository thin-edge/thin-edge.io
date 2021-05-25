use std::sync::Arc;

use log::{debug, error, info};
use mqtt_client::{Client, Config, Message, Topic};
use software_management::message::*;
use software_management::plugin::*;
use software_management::plugin_manager::*;
use software_management::software::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let name = "sm-agent";
    let operation_topic = Topic::new("tedge/sm-operations")?;
    let status_topic = Topic::new("tedge/sm-status")?;
    let error_topic = Topic::new("tedge/sm-errors")?;
    let plugins = Arc::new(ExternalPlugins::open("/etc/tedge/sm-plugins")?);

    env_logger::init();
    info!("Starting SM-Agent");

    let mqtt = Client::connect(name, &Config::default()).await?;
    let mut errors = mqtt.subscribe_errors();
    tokio::spawn(async move {
        while let Some(error) = errors.next().await {
            error!("{}", error);
        }
    });

    let mut operations = mqtt.subscribe(operation_topic.filter()).await?;
    while let Some(message) = operations.next().await {
        debug!("Request {:?}", message);

        let payload = match String::from_utf8(message.payload) {
            Ok(utf8) => utf8,
            Err(error) => {
                debug!("UTF8 error: {}", error);
                let _ = mqtt
                    .publish(Message::new(&error_topic, format!("{}", error)))
                    .await?;
                continue;
            }
        };

        let request = match SoftwareRequest::from_json(&payload) {
            Ok(request) => request,
            Err(error) => {
                debug!("Parsing error: {}", error);
                let _ = mqtt
                    .publish(Message::new(&error_topic, format!("{}", error)))
                    .await?;
                continue;
            }
        };

        match request.operation {
            SoftwareOperation::CurrentSoftwareList => {
                unimplemented!();
            }

            SoftwareOperation::SoftwareUpdates { updates } => {
                for update in &updates {
                    let status = SoftwareUpdateStatus::scheduled(update);
                    let json = serde_json::to_string(&status)?;
                    let _ = mqtt.publish(Message::new(&status_topic, json)).await?;
                }

                for update in updates {
                    let plugins = plugins.clone();
                    let blocking_task = tokio::task::spawn_blocking(move || plugins.apply(&update));
                    let status: SoftwareUpdateStatus = blocking_task.await?;
                    let json = serde_json::to_string(&status)?;
                    let _ = mqtt.publish(Message::new(&status_topic, json)).await?;
                }
            }

            SoftwareOperation::DesiredSoftwareList { modules: _ } => {
                unimplemented!();
            }
        }
    }

    Ok(())
}
