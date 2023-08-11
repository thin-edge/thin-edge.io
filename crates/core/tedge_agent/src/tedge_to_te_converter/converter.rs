use log::error;
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
        Ok(self.wrap_errors(messages_or_err))
    }
}

impl TedgetoTeConverter {
    pub fn new() -> Self {
        TedgetoTeConverter {}
    }

    fn try_convert(
        &mut self,
        message: MqttMessage,
    ) -> Result<Vec<tedge_mqtt_ext::Message>, serde_json::Error> {
        match message.topic.clone() {
            topic if topic.name.starts_with("tedge/measurements") => {
                Ok(self.convert_measurement(message))
            }
            topic if topic.name.starts_with("tedge/events") => Ok(self.convert_event(message)),
            topic if topic.name.starts_with("tedge/alarms") => self.convert_alarm(message),
            topic if topic.name.starts_with("tedge/health") => {
                Ok(self.convert_health_status_message(message))
            }
            _ => Ok(vec![]),
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
    fn convert_alarm(
        &mut self,
        mut message: MqttMessage,
    ) -> Result<Vec<MqttMessage>, serde_json::Error> {
        let (te_topic, severity) = match message.topic.name.split('/').collect::<Vec<_>>()[..] {
            ["tedge", "alarms", severity, alarm_type] => (
                Topic::new_unchecked(format!("te/device/main///a/{alarm_type}").as_str()),
                severity,
            ),
            ["tedge", "alarms", severity, alarm_type, cid] => (
                Topic::new_unchecked(format!("te/device/{cid}///a/{alarm_type}").as_str()),
                severity,
            ),
            _ => return Ok(vec![]),
        };

        let mut alarm: HashMap<String, Value> = serde_json::from_slice(message.payload.as_bytes())?;
        alarm.insert("severity".to_string(), severity.into());
        message.topic = te_topic;
        message.payload = serde_json::to_string(&alarm)?.into();
        message.retain = true;
        Ok(vec![message])
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

    fn wrap_errors(
        &self,
        messages_or_err: Result<Vec<MqttMessage>, serde_json::Error>,
    ) -> Vec<MqttMessage> {
        messages_or_err.unwrap_or_else(|error| vec![self.new_error_message(error)])
    }

    fn new_error_message(&self, error: serde_json::Error) -> MqttMessage {
        error!("Mapping error: {}", error);
        MqttMessage::new(&Topic::new_unchecked("tedge/errors"), error.to_string())
    }
}
