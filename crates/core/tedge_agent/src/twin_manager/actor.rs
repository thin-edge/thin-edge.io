use crate::twin_manager::builder::TwinManagerConfig;
use async_trait::async_trait;
use serde_json::Map;
use serde_json::Value;
use std::fs::File;
use std::time::Duration;
use tedge_actors::Actor;
use tedge_actors::LoggingSender;
use tedge_actors::MessageReceiver;
use tedge_actors::RuntimeError;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_mqtt_ext::MqttMessage;
use tokio::time::timeout;
use tracing::error;

const INVENTORY_FRAGMENTS_FILE_LOCATION: &str = "device/inventory.json";

pub struct TwinManagerActor {
    config: TwinManagerConfig,
    messages: SimpleMessageBox<MqttMessage, MqttMessage>,
    mqtt_publisher: LoggingSender<MqttMessage>,
}

#[async_trait]
impl Actor for TwinManagerActor {
    fn name(&self) -> &str {
        "TwinManagerActor"
    }

    async fn run(mut self) -> Result<(), RuntimeError> {
        let mut inventory_map = self.load_inventory_json()?;
        // Wait until the very fist message is received (at least the agent health status is guaranteed)
        if let Some(mut msg) = self.messages.recv().await {
            loop {
                if let Ok((_, Channel::EntityTwinData { fragment_key })) = self
                    .config
                    .mqtt_schema
                    .entity_channel_of(msg.topic.as_ref())
                {
                    // If a twin data message for the same key is available,
                    // ignore the value in inventory JSON
                    inventory_map.remove(&fragment_key);
                }

                // If no more messages are available for up to 1 second, that's taken as a cue that
                // all existing twin messages have been delivered and break out of the loop
                msg = match timeout(Duration::from_secs(1), self.messages.recv()).await {
                    Ok(Some(next)) => next,
                    _ => break,
                };
            }
        }

        // Publish any remaining twin data loaded from the inventory JSON file
        let device_id = self.config.device_topic_id.clone();
        for (key, value) in inventory_map {
            self.publish_twin_data(&device_id, key, value).await;
        }

        // This is to prevent the MQTT actor from crashing while trying to send a twin message to this actor later
        // after this actor has finished and closed its message box, but the MQTT actor still holds the sender half of it.
        while self.messages.recv().await.is_some() {
            // Do nothing
        }
        Ok(())
    }
}

impl TwinManagerActor {
    pub fn new(
        config: TwinManagerConfig,
        messages: SimpleMessageBox<MqttMessage, MqttMessage>,
        mqtt_publisher: LoggingSender<MqttMessage>,
    ) -> Self {
        Self {
            config,
            messages,
            mqtt_publisher,
        }
    }

    fn load_inventory_json(&self) -> Result<Map<String, Value>, RuntimeError> {
        let inventory_file_path = self
            .config
            .config_dir
            .join(INVENTORY_FRAGMENTS_FILE_LOCATION);
        let file = match File::open(&inventory_file_path) {
            Ok(file) => file,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Map::new());
            }
            Err(err) => {
                return Err(RuntimeError::ActorError(
                    format!(
                        "Failed to open inventory file at {} with {}",
                        inventory_file_path, err
                    )
                    .into(),
                ));
            }
        };
        let inventory_json: Value = serde_json::from_reader(file).map_err(|err| {
            RuntimeError::ActorError(
                format!(
                    "Failed to parse the contents of inventory file {} as JSON: {}",
                    inventory_file_path, err
                )
                .into(),
            )
        })?;
        let Value::Object(twin_map) = inventory_json else {
            return Err(RuntimeError::ActorError(
                format!(
                    "Invalid inventory.json format: expected a JSON object in {}",
                    inventory_file_path
                )
                .into(),
            ));
        };
        Ok(twin_map)
    }

    async fn publish_twin_data(
        &mut self,
        topic_id: &EntityTopicId,
        fragment_key: String,
        fragment_value: Value,
    ) {
        let twin_channel = Channel::EntityTwinData { fragment_key };
        let topic = self.config.mqtt_schema.topic_for(topic_id, &twin_channel);
        let payload = if fragment_value.is_null() {
            "".to_string()
        } else {
            fragment_value.to_string()
        };
        let message = MqttMessage::new(&topic, payload).with_retain();
        self.publish_message(message).await;
    }

    async fn publish_message(&mut self, message: MqttMessage) {
        let topic = message.topic.clone();
        if let Err(err) = self.mqtt_publisher.send(message).await {
            error!("Failed to publish the message on topic: {topic:?} due to {err}");
        }
    }
}
