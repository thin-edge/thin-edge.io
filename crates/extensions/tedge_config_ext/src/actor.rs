use async_trait::async_trait;
use serde_json::Value;
use std::collections::BTreeMap;
use tedge_actors::Actor;
use tedge_actors::LoggingSender;
use tedge_actors::MessageReceiver;
use tedge_actors::RuntimeError;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::ServiceTopicId;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::QoS;
use tracing::error;

pub struct ConfigPublisherActor {
    mqtt_schema: MqttSchema,
    service_topic_id: ServiceTopicId,
    /// Every currently-set exposed key-value pair, published as one retained JSON object. Each
    /// value keeps the type it has in `tedge.toml`, so a port stays a number and a flag stays a
    /// boolean. A key that is exposable but unset is simply absent here.
    expected: BTreeMap<String, Value>,
    messages: SimpleMessageBox<MqttMessage, MqttMessage>,
    mqtt_publisher: LoggingSender<MqttMessage>,
}

impl ConfigPublisherActor {
    pub fn new(
        mqtt_schema: MqttSchema,
        service_topic_id: ServiceTopicId,
        exposed_config: Vec<(String, Option<Value>)>,
        messages: SimpleMessageBox<MqttMessage, MqttMessage>,
        mqtt_publisher: LoggingSender<MqttMessage>,
    ) -> Self {
        let expected = exposed_config
            .into_iter()
            .filter_map(|(key, value)| Some((key, value?)))
            .collect();

        Self {
            mqtt_schema,
            service_topic_id,
            expected,
            messages,
            mqtt_publisher,
        }
    }
}

#[async_trait]
impl Actor for ConfigPublisherActor {
    fn name(&self) -> &str {
        "ConfigPublisherActor"
    }

    async fn run(mut self) -> Result<(), RuntimeError> {
        self.publish_current_config().await;

        while let Some(message) = self.messages.recv().await {
            self.reconcile(message).await;
        }

        Ok(())
    }
}

impl ConfigPublisherActor {
    /// Reconciles a message received on this actor's own `config` topic against the expected
    /// JSON object, republishing it whenever the retained payload doesn't match. Because the
    /// whole document is replaced as a unit rather than patched key by key, this same
    /// comparison also clears a stale key left behind by a rename, removal, or demotion — it is
    /// simply absent from the republished object.
    async fn reconcile(&mut self, message: MqttMessage) {
        let Ok((_, Channel::Config)) = self.mqtt_schema.entity_channel_of(&message.topic) else {
            return;
        };

        if !self.payload_matches_expected(message.payload_bytes()) {
            self.publish_current_config().await;
        }
    }

    fn payload_matches_expected(&self, payload: &[u8]) -> bool {
        serde_json::from_slice::<BTreeMap<String, Value>>(payload)
            .is_ok_and(|received| received == self.expected)
    }

    async fn publish_current_config(&mut self) {
        let topic = self
            .mqtt_schema
            .topic_for(self.service_topic_id.entity(), &Channel::Config);
        let payload =
            serde_json::to_string(&self.expected).expect("a map of JSON values always serializes");
        let message = MqttMessage::new(&topic, payload)
            .with_retain()
            .with_qos(QoS::AtLeastOnce);

        if let Err(err) = self.mqtt_publisher.send(message).await {
            error!("Failed to publish the config document due to {err}");
        }
    }
}
