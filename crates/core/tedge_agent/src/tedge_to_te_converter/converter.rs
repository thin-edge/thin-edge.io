use serde_json::Value;
use std::collections::HashMap;
use std::convert::Infallible;
use tedge_actors::Converter;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;

pub struct TedgetoTeConverter {}

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
    pub fn new() -> Self {
        TedgetoTeConverter {}
    }

    fn try_convert(&mut self, message: MqttMessage) -> Vec<tedge_mqtt_ext::MqttMessage> {
        match message.topic.clone() {
            topic if topic.name.starts_with("tedge/measurements") => {
                self.convert_measurement(message)
            }
            topic if topic.name.starts_with("tedge/events") => self.convert_event(message),
            topic if topic.name.starts_with("tedge/alarms") => self.convert_alarm(message),
            _ => vec![],
        }
    }

    // tedge/measurements -> te/device/main///m/
    // tedge/measurements/child -> te/device/child///m/
    fn convert_measurement(
        &mut self,
        mut message: MqttMessage,
    ) -> Vec<tedge_mqtt_ext::MqttMessage> {
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
    fn convert_event(&mut self, mut message: MqttMessage) -> Vec<tedge_mqtt_ext::MqttMessage> {
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
}
#[cfg(test)]
mod tests {
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
}
