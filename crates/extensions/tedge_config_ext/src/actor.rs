use async_trait::async_trait;
use std::collections::HashMap;
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
    /// Every exposable key in scope, with the value to publish at startup (or `None` to clear
    /// any stale retained message left over from a previous run).
    initial_config: Vec<(String, Option<String>)>,
    /// The expected value for each key that is currently exposed and set. A key that is exposed
    /// but unset, or not in the exposed set at all, has no entry here, and its expected state is
    /// "absent" (i.e. no retained message).
    expected: HashMap<String, String>,
    messages: SimpleMessageBox<MqttMessage, MqttMessage>,
    mqtt_publisher: LoggingSender<MqttMessage>,
}

impl ConfigPublisherActor {
    pub fn new(
        mqtt_schema: MqttSchema,
        service_topic_id: ServiceTopicId,
        exposed_config: Vec<(String, Option<String>)>,
        messages: SimpleMessageBox<MqttMessage, MqttMessage>,
        mqtt_publisher: LoggingSender<MqttMessage>,
    ) -> Self {
        let expected = exposed_config
            .iter()
            .filter_map(|(key, value)| Some((key.clone(), value.clone()?)))
            .collect();

        Self {
            mqtt_schema,
            service_topic_id,
            initial_config: exposed_config,
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
        // Startup pass: publish every exposable key in scope, clearing any that are unset. This
        // is required even for keys the reconciliation loop below could otherwise handle, since
        // reconciliation only reacts to messages it receives, and a brand new key has no retained
        // message yet to react to.
        let initial_config = std::mem::take(&mut self.initial_config);
        for (key, value) in initial_config {
            self.publish_config_value(&key, value.as_deref()).await;
        }

        while let Some(message) = self.messages.recv().await {
            self.reconcile(message).await;
        }

        Ok(())
    }
}

impl ConfigPublisherActor {
    /// Reconciles a message received on this actor's own `config/+` topics against the expected
    /// state for that key: republishes a diverged (or wrongly-cleared) owned value, clears a
    /// retained message for a key that is no longer exposed (renamed, removed, or demoted), and
    /// otherwise does nothing, so the actor's own echo does not trigger a further action.
    async fn reconcile(&mut self, message: MqttMessage) {
        let Ok((_, Channel::Config { key })) = self.mqtt_schema.entity_channel_of(&message.topic)
        else {
            return;
        };
        let payload = message.payload_str().unwrap_or_default();

        match self.expected.get(&key).cloned() {
            Some(expected_value) if payload != expected_value => {
                self.publish_config_value(&key, Some(&expected_value)).await;
            }
            None if !payload.is_empty() => {
                self.publish_config_value(&key, None).await;
            }
            _ => {
                // The payload already matches the expected state: either it is the owned value
                // this actor just published, or it is an empty payload on a key that has no
                // expected value at all. Either way, no action is taken.
            }
        }
    }

    async fn publish_config_value(&mut self, key: &str, value: Option<&str>) {
        let topic = self.mqtt_schema.topic_for(
            self.service_topic_id.entity(),
            &Channel::Config {
                key: key.to_string(),
            },
        );
        let message = MqttMessage::new(&topic, value.unwrap_or(""))
            .with_retain()
            .with_qos(QoS::AtLeastOnce);

        if let Err(err) = self.mqtt_publisher.send(message).await {
            error!("Failed to publish the config value for '{key}' due to {err}");
        }
    }
}
