use serde::Deserialize;
use serde::Serialize;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_mqtt_ext::MqttMessage;
use time::OffsetDateTime;

#[derive(Deserialize, Serialize)]
pub struct C8yEntityBirth {
    pub topic: String,
    pub status: C8yEntityStatus,
    pub time: f64,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum C8yEntityStatus {
    Registered,
    Unregistered,
}

impl C8yEntityBirth {
    pub fn birth_message(
        te: &MqttSchema,
        mapper: &EntityTopicId,
        entity: &EntityTopicId,
    ) -> MqttMessage {
        let message_topic = te.topic_for(
            mapper,
            &Channel::Status {
                component: "entities".to_string(),
            },
        );
        let entity_topic = te.topic_for(entity, &Channel::EntityMetadata);
        let birth = C8yEntityBirth {
            topic: entity_topic.to_string(),
            status: C8yEntityStatus::Registered,
            time: OffsetDateTime::now_utc().unix_timestamp_nanos() as f64 / 1e9,
        };
        MqttMessage::new(&message_topic, birth.to_json())
    }

    pub fn from_json(payload: &str) -> Result<Self, String> {
        serde_json::from_str(payload)
            .map_err(|e| format!("Not a C8Y entity registration message: {:?}", e))
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap()
    }
}
