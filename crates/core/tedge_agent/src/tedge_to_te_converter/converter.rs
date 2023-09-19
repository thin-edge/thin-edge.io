use serde_json::Value;
use std::collections::HashMap;
use std::convert::Infallible;
use tedge_actors::Converter;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;

pub struct TedgetoTeConverter {
    mqtt_schema: MqttSchema,
}

impl Converter for TedgetoTeConverter {
    type Input = MqttMessage;
    type Output = MqttMessage;
    type Error = Infallible;

    fn convert(&mut self, input: &Self::Input) -> Result<Vec<Self::Output>, Self::Error> {
        let messages_or_err = self.try_convert(input.clone());
        Ok(messages_or_err)
    }
}

impl TedgetoTeConverter {
    /// Creates a new converter with default prefix `"te"`.
    #[cfg(test)]
    pub fn new() -> Self {
        Self {
            mqtt_schema: MqttSchema::new(),
        }
    }

    pub fn with_root(root: String) -> Self {
        Self {
            mqtt_schema: MqttSchema::with_root(root),
        }
    }

    fn try_convert(&mut self, message: MqttMessage) -> Vec<tedge_mqtt_ext::Message> {
        match &message.topic {
            topic if topic.name.starts_with("tedge/measurements") => {
                self.convert_measurement(message)
            }
            topic if topic.name.starts_with("tedge/events") => self.convert_event(message),
            topic if topic.name.starts_with("tedge/alarms") => self.convert_alarm(message),

            // to be able to move different services to new health topic at different times, we
            // selectively map services either from old to new topics, and vice versa.
            // tedge-agent publishes on new health topic, exclude it from old mapping not to produce
            // a publish loop
            topic
                if topic.name.starts_with("tedge/health")
                    && !topic.name.contains("tedge-agent") =>
            {
                self.convert_health_status_message(message)
            }

            // Convert messages from new topics to old topics for backward compatibility
            topic if topic.name.starts_with(&self.mqtt_schema.root) => {
                match self.mqtt_schema.entity_channel_of(topic) {
                    Ok((entity_topic_id, Channel::Health))
                        if entity_topic_id.as_str().contains("tedge-agent") =>
                    {
                        self.convert_new_health_status_message_to_old(message)
                    }
                    Ok(_) => vec![],
                    Err(_) => vec![],
                }
            }

            topic if topic.name.starts_with("tedge/health-check") => {
                self.convert_health_check_command(message)
            }

            _ => vec![],
        }
    }

    // tedge/measurements -> te/device/main///m/
    // tedge/measurements/child -> te/device/child///m/
    fn convert_measurement(&mut self, mut message: MqttMessage) -> Vec<tedge_mqtt_ext::Message> {
        let te_topic = match message.topic.name.split('/').collect::<Vec<_>>()[..] {
            ["tedge", "measurements"] => Topic::new_unchecked("te/device/main///m/"),
            ["tedge", "measurements", cid] => {
                Topic::new_unchecked(format!("te/device/{cid}///m/").as_str())
            }
            _ => return vec![],
        };

        message.topic = te_topic;
        vec![(message)]
    }

    // tedge/alarms/severity/alarm_type -> te/device/main///a/alarm_type, put severity in payload
    // tedge/alarms/severity/alarm_type/child ->  te/device/child///a/alarm_type, put severity in payload
    fn convert_alarm(&mut self, mut message: MqttMessage) -> Vec<MqttMessage> {
        let (te_topic, severity) = match message.topic.name.split('/').collect::<Vec<_>>()[..] {
            ["tedge", "alarms", severity, alarm_type] => (
                Topic::new_unchecked(format!("te/device/main///a/{alarm_type}").as_str()),
                severity,
            ),
            ["tedge", "alarms", severity, alarm_type, cid] => (
                Topic::new_unchecked(format!("te/device/{cid}///a/{alarm_type}").as_str()),
                severity,
            ),
            _ => return vec![],
        };

        // if alarm payload is empty, then it's a clear alarm message. So, forward empty payload
        // if the alarm payload is not empty then update the severity.
        if !message.payload().is_empty() {
            let res: Result<HashMap<String, Value>, serde_json::Error> =
                serde_json::from_slice(message.payload.as_bytes());
            if let Ok(mut alarm) = res {
                alarm.insert("severity".to_string(), severity.into());
                // serialize the payload after updating the severity
                if let Ok(payload) = serde_json::to_string(&alarm) {
                    message.payload = payload.into()
                }
            }
        }
        message.topic = te_topic;
        message.retain = true;
        vec![message]
    }

    // tedge/events/event_type -> te/device/main///e/event_type
    // tedge/events/event_type/child -> te/device/child///e/event_type
    fn convert_event(&mut self, mut message: MqttMessage) -> Vec<tedge_mqtt_ext::Message> {
        let topic = match message.topic.name.split('/').collect::<Vec<_>>()[..] {
            ["tedge", "events", event_type] => {
                Topic::new_unchecked(format!("te/device/main///e/{event_type}").as_str())
            }
            ["tedge", "events", event_type, cid] => {
                Topic::new_unchecked(format!("te/device/{cid}///e/{event_type}").as_str())
            }
            _ => return vec![],
        };

        message.topic = topic;
        vec![message]
    }

    // tedge/health/service-name -> te/device/main/service/<service-name>/status/health
    // tedge/health/child/service-name -> te/device/child/service/<service-name>/status/health
    fn convert_health_status_message(&mut self, mut message: MqttMessage) -> Vec<MqttMessage> {
        let topic = match message.topic.name.split('/').collect::<Vec<_>>()[..] {
            ["tedge", "health", service_name] => Topic::new_unchecked(
                format!("te/device/main/service/{service_name}/status/health").as_str(),
            ),
            ["tedge", "health", cid, service_name] => Topic::new_unchecked(
                format!("te/device/{cid}/service/{service_name}/status/health").as_str(),
            ),
            _ => return vec![],
        };
        message.topic = topic;
        message.retain = true;
        vec![message]
    }

    /// Maps health messages from a new topic scheme to the old.
    ///
    /// The `message` is assumed to be a health message coming from a service under a new topic
    /// scheme. This function should be called for services already ported to the new topic scheme
    /// and these services should be excluded from mapping old -> new, or else a message outputted
    /// by this function will cause `convert_health_status_message`, which in turn will output
    /// a message which will cause this function to be called again, resultin in a loop.
    fn convert_new_health_status_message_to_old(
        &mut self,
        mut message: MqttMessage,
    ) -> Vec<MqttMessage> {
        // message fits new topic scheme
        let topic = message.topic;
        let (entity_topic_id, channel) = self
            .mqtt_schema
            .entity_channel_of(&topic)
            .expect("topic should be confirmed to fit new schema in try_convert");

        if channel != Channel::Health {
            return vec![];
        }

        // TODO: move topic schema mapping into tedge-api
        let topic = match entity_topic_id.as_str().split('/').collect::<Vec<&str>>()[..] {
            ["device", "main", "service", service_name] => format!("tedge/health/{service_name}"),
            ["device", cid, "service", service_name] => {
                format!("tedge/health/{cid}/{service_name}")
            }
            // topics which do not fit a default schema are not mapped
            _ => return vec![],
        };

        let topic = Topic::new_unchecked(&topic);
        message.topic = topic;
        message.retain = true;
        vec![message]
    }

    fn convert_health_check_command(
        &self,
        mut message: tedge_mqtt_ext::Message,
    ) -> Vec<tedge_mqtt_ext::Message> {
        let topic = match message.topic.name.split('/').collect::<Vec<_>>()[..] {
            ["tedge", "health-check"] => Topic::new_unchecked("te/device/main///cmd/health/check"),
            ["tedge", "health-check", service_name] => Topic::new_unchecked(
                format!("te/device/main/service/{service_name}/cmd/health/check").as_str(),
            ),
            _ => return vec![],
        };
        message.topic = topic;
        message.retain = true;
        vec![message]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::tedge_to_te_converter::converter::TedgetoTeConverter;
    use tedge_mqtt_ext::MqttMessage;
    use tedge_mqtt_ext::Topic;

    #[test]
    fn convert_incoming_wrong_topic() {
        let mqtt_message = MqttMessage::new(&Topic::new_unchecked("tedge///MyCustomAlarm"), "");
        let mut converter = TedgetoTeConverter::new();
        let res = converter.try_convert(mqtt_message);
        assert!(res.is_empty())
    }

    /// Ensures that private method `convert_new_health_status_message_to_old` only converts new
    /// health messages to old messages for updated components.
    // this test will have to be altered as components are updated to work with new health topics
    #[test]
    fn converts_health_status_messages_for_agent() {
        let mut converter = TedgetoTeConverter::new();

        let entities_incorrect = [
            "device/main/service/other-service",
            "device/child001/service/other-service",
            "factory01/hallA/packaging/belt001",
        ];

        for topic in entities_incorrect {
            let topic = converter
                .mqtt_schema
                .topic_for(&topic.parse().unwrap(), &Channel::Health);
            let message = MqttMessage::new(&topic, "");

            assert_eq!(converter.convert(&message).unwrap(), vec![]);
        }

        let topic = converter.mqtt_schema.topic_for(
            &"device/main/service/tedge-agent".parse().unwrap(),
            &Channel::Health,
        );
        let message = MqttMessage::new(&topic, "");
        let expected_topic = "tedge/health/tedge-agent";
        let messages = converter.convert(&message).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].topic.name, expected_topic);

        let topic = converter.mqtt_schema.topic_for(
            &"device/child001/service/tedge-agent".parse().unwrap(),
            &Channel::Health,
        );
        let message = MqttMessage::new(&topic, "");
        let expected_topic = "tedge/health/child001/tedge-agent";
        let messages = converter.convert(&message).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].topic.name, expected_topic);
    }
}
