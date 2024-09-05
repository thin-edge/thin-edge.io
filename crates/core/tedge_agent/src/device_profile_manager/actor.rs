use async_trait::async_trait;
use log::info;
use tedge_actors::Actor;
use tedge_actors::LoggingSender;
use tedge_actors::MessageReceiver;
use tedge_actors::RuntimeError;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use tedge_api::device_profile::DeviceProfileCmd;
use tedge_api::device_profile::DeviceProfileInfo;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::CommandStatus;
use tedge_api::Jsonify;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::QoS;

pub struct DeviceProfileManagerActor {
    message_box: SimpleMessageBox<DeviceProfileCmd, DeviceProfileCmd>,
    mqtt_schema: MqttSchema,
    mqtt_publisher: LoggingSender<MqttMessage>,
}

#[async_trait]
impl Actor for DeviceProfileManagerActor {
    fn name(&self) -> &str {
        "DeviceProfileManagerActor"
    }

    async fn run(mut self) -> Result<(), RuntimeError> {
        while let Some(command) = self.message_box.recv().await {
            info!("Command received: {:?}", &command);
            if command.status() == CommandStatus::Successful {
                let twin_metadata_topic = self.mqtt_schema.topic_for(
                    &command.target,
                    &Channel::EntityTwinData {
                        fragment_key: "device_profile".to_string(),
                    },
                );

                let twin_metadata_payload = DeviceProfileInfo {
                    name: Some(command.payload.name.clone()),
                    version: None,
                };

                let twin_metadata =
                    MqttMessage::new(&twin_metadata_topic, twin_metadata_payload.to_json())
                        .with_retain()
                        .with_qos(QoS::AtLeastOnce);

                self.mqtt_publisher.send(twin_metadata).await?;
            }
        }

        Ok(())
    }
}

impl DeviceProfileManagerActor {
    pub fn new(
        message_box: SimpleMessageBox<DeviceProfileCmd, DeviceProfileCmd>,
        mqtt_schema: MqttSchema,
        mqtt_publisher: LoggingSender<MqttMessage>,
    ) -> Self {
        Self {
            message_box,
            mqtt_schema,
            mqtt_publisher,
        }
    }
}
