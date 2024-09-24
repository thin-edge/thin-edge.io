use std::collections::HashMap;

use crate::mqtt_topics::Channel;
use crate::mqtt_topics::MqttSchema;
use crate::Status;
use mqtt_channel::MqttMessage;
use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde_json::Value as JsonValue;

use super::HealthTopicError;

/// Payload of the health status message.
///
/// https://thin-edge.github.io/thin-edge.io/operate/troubleshooting/monitoring-service-health/
#[derive(Deserialize, Serialize, Debug, Default)]
#[serde(from = "HealthStatusImp")]
pub struct HealthStatus {
    /// Current status of the service, synced by the mapper to the cloud
    pub status: Status,

    /// Used by the watchdog to monitor restarts of services.
    ///
    /// None if value not present or could not be deserialized successfully.
    pub pid: Option<u32>,

    pub time: Option<JsonValue>,

    #[serde(flatten)]
    pub extra: HashMap<String, JsonValue>,
}

impl HealthStatus {
    pub fn try_from_health_status_message(
        message: &MqttMessage,
        mqtt_schema: &MqttSchema,
    ) -> Result<Self, HealthTopicError> {
        if let Ok((topic_id, Channel::Health)) = mqtt_schema.entity_channel_of(&message.topic) {
            let health_status = if super::entity_is_mosquitto_bridge_service(&topic_id) {
                let status = match message.payload_str() {
                    Ok("1") => Status::Up,
                    Ok("0") => Status::Down,
                    _ => Status::default(),
                };
                HealthStatus {
                    status,
                    pid: None,
                    time: None,
                    extra: HashMap::new(),
                }
            } else {
                serde_json::from_slice(message.payload()).unwrap_or_default()
            };
            Ok(health_status)
        } else {
            Err(HealthTopicError)
        }
    }

    pub fn is_valid(&self) -> bool {
        self.status == Status::Up || self.status == Status::Down
    }
}

/// Provide customised Serialize/Deserialize implementations while exposing simple structure for tedge_api consumers.
#[derive(Deserialize, Serialize, Debug)]
struct HealthStatusImp {
    status: Status,
    pid: NoneIfErr<u32>,
    time: Option<JsonValue>,
    #[serde(flatten)]
    pub extra: HashMap<String, JsonValue>,
}

impl From<HealthStatusImp> for HealthStatus {
    fn from(value: HealthStatusImp) -> Self {
        HealthStatus {
            status: value.status,
            pid: value.pid.0,
            time: value.time,
            extra: value.extra,
        }
    }
}

/// Deserialize to `None` if deserialization fails.
///
/// We want to be able to use required fields like `status` even if there are errors when serializing some other
/// optional fields (e.g. PID).
#[derive(Serialize, Debug, Default)]
#[serde(transparent)]
struct NoneIfErr<T>(Option<T>);

impl<'de, T: Deserialize<'de>> Deserialize<'de> for NoneIfErr<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(NoneIfErr(T::deserialize(deserializer).ok()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::json;

    #[test]
    fn pid_none_when_missing() {
        let health_status =
            serde_json::from_value::<HealthStatus>(json!({"status": "up"})).unwrap();
        assert_eq!(health_status.pid, None);
    }

    #[test]
    fn pid_some_when_correct_type() {
        let health_status =
            serde_json::from_value::<HealthStatus>(json!({"status": "up", "pid": 2137})).unwrap();
        assert_eq!(health_status.pid, Some(2137));
    }

    #[test]
    fn pid_none_when_incorrect_type() {
        let health_status =
            serde_json::from_value::<HealthStatus>(json!({"status": "up", "pid": "invalid type"}))
                .unwrap();
        assert_eq!(health_status.pid, None);
    }
}
