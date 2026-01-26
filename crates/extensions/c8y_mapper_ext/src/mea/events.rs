use crate::entity_cache::CloudEntityMetadata;
use crate::mea::get_entity_metadata;
use crate::mea::take_cached_telemetry_data;
use c8y_api::json_c8y::C8yCreateEvent;
use c8y_api::smartrest::topic::C8yTopic;
use std::time::SystemTime;
use tedge_api::entity::EntityExternalId;
use tedge_api::event::error::ThinEdgeJsonDeserializerError;
use tedge_api::event::ThinEdgeEvent;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::store::RingBuffer;
use tedge_config::models::TopicPrefix;
use tedge_flows::ConfigError;
use tedge_flows::FlowContextHandle;
use tedge_flows::FlowError;
use tedge_flows::JsonValue;
use tedge_flows::Message;
use tedge_mqtt_ext::TopicFilter;

const DEFAULT_EVENT_TYPE: &str = "ThinEdgeEvent";
const C8Y_JSON_MQTT_EVENTS_TOPIC: &str = "event/events/create";
const C8Y_JSON_HTTP_EVENTS_TOPIC: &str = "http/events/create";
const CREATE_EVENT_SMARTREST_CODE: u16 = 400;

#[derive(Clone)]
pub struct EventConverter {
    mqtt_schema: MqttSchema,
    c8y_prefix: TopicPrefix,
    max_mqtt_payload_size: Option<usize>,
    cache: RingBuffer<Message>,
}

impl Default for EventConverter {
    fn default() -> Self {
        EventConverter {
            mqtt_schema: MqttSchema::default(),
            c8y_prefix: TopicPrefix::try_new("c8y").unwrap(),
            max_mqtt_payload_size: None,
            cache: RingBuffer::default(),
        }
    }
}

impl tedge_flows::Transformer for EventConverter {
    fn name(&self) -> &str {
        "into_c8y_events"
    }

    fn set_config(&mut self, config: JsonValue) -> Result<(), ConfigError> {
        if let Some(root) = config.string_property("topic_root") {
            self.mqtt_schema = MqttSchema::with_root(root.to_string())
        }
        if let Some(c8y_prefix) = config.string_property("c8y_prefix") {
            self.c8y_prefix = TopicPrefix::try_new(c8y_prefix).map_err(|err| {
                ConfigError::IncorrectSetting(format!("Not a valid c8y topic prefix: {}", err))
            })?;
        }
        self.max_mqtt_payload_size = config
            .number_property("max_mqtt_payload_size")
            .and_then(|n| n.as_u64())
            .map(|n| n as usize);
        Ok(())
    }

    fn on_message(
        &mut self,
        _timestamp: SystemTime,
        message: &Message,
        context: &FlowContextHandle,
    ) -> Result<Vec<Message>, FlowError> {
        match self.mqtt_schema.entity_channel_of(&message.topic) {
            Ok((entity_id, Channel::Event { event_type })) => {
                let Some(entity) = get_entity_metadata(context, entity_id.as_str()) else {
                    self.cache.push(message.clone());
                    return Ok(vec![]);
                };
                let event = self.convert(entity, &event_type, message)?;
                Ok(vec![event])
            }
            Ok((_, Channel::Status { component })) if component == "entities" => {
                self.process_cached_messages(context, message)
            }
            _ => Err(FlowError::UnsupportedMessage(format!(
                "Not an event topic: {}",
                message.topic
            ))),
        }
    }
}

impl EventConverter {
    fn convert(
        &mut self,
        entity: CloudEntityMetadata,
        event_type: &str,
        event: &Message,
    ) -> Result<Message, FlowError> {
        let event_type = match event_type.is_empty() {
            true => DEFAULT_EVENT_TYPE,
            false => event_type,
        };

        let mqtt_payload = event.payload_str().ok_or_else(|| {
            FlowError::UnsupportedMessage(format!(
                "Not an UTF8 event payload received on: {}",
                event.topic
            ))
        })?;

        let tedge_event = ThinEdgeEvent::try_from(
            event_type,
            &entity.metadata.r#type,
            &entity.external_id,
            mqtt_payload,
        )
        .map_err(
            |e| ThinEdgeJsonDeserializerError::FailedToParseJsonPayload {
                topic: event.topic.to_string(),
                error: e.to_string(),
                payload: mqtt_payload.chars().take(50).collect(),
            },
        )
        .map_err(|e| FlowError::UnsupportedMessage(format!("Not an event payload: {e}")))?;

        let c8y_event = C8yCreateEvent::from(tedge_event);

        let message = if c8y_event.extras.is_empty() {
            // If the message doesn't contain any fields other than `text` and `time`, convert to SmartREST
            let smartrest_event = Self::serialize_to_smartrest(&c8y_event)?;
            let smartrest_topic = C8yTopic::upstream_topic(&self.c8y_prefix);
            Message::new(&smartrest_topic, smartrest_event)
        } else {
            // If the message contains extra fields other than `text` and `time`, convert to Cumulocity JSON
            let cumulocity_event_json = serde_json::to_string(&c8y_event).map_err(|e| {
                FlowError::UnsupportedMessage(format!("Fail to format format event as JSON: {e}"))
            })?;
            let json_mqtt_topic = &format!("{}/{C8Y_JSON_MQTT_EVENTS_TOPIC}", self.c8y_prefix);
            Message::new(json_mqtt_topic, cumulocity_event_json)
        };

        if self.can_send_over_mqtt(&message) {
            // The message can be sent via MQTT
            Ok(message)
        } else {
            // The message must be sent over HTTP
            // Actually this converter forwards this message over MQTT to the c8y converter which does the HTTP request
            let http_event = serde_json::to_string(&c8y_event).map_err(|e| {
                FlowError::UnsupportedMessage(format!("Fail to format format event as JSON: {e}"))
            })?;
            let http_topic = self.http_event_topic(&entity.external_id);

            Ok(Message::new(http_topic, http_event))
        }
    }

    pub fn http_event_topic_filter(c8y_prefix: &TopicPrefix) -> TopicFilter {
        TopicFilter::new_unchecked(&format!("{c8y_prefix}/{C8Y_JSON_HTTP_EVENTS_TOPIC}/+"))
    }

    fn http_event_topic(&self, device: &EntityExternalId) -> String {
        format!(
            "{}/{C8Y_JSON_HTTP_EVENTS_TOPIC}/{}",
            self.c8y_prefix,
            device.as_ref()
        )
    }

    fn serialize_to_smartrest(c8y_event: &C8yCreateEvent) -> Result<String, FlowError> {
        let time = c8y_event
            .time
            .format(&time::format_description::well_known::Rfc3339)
            .map_err(|e| {
                FlowError::UnsupportedMessage(format!("Fail to format timestamp as Rfc3339: {e}"))
            })?;

        Ok(format!(
            "{},{},\"{}\",{}",
            CREATE_EVENT_SMARTREST_CODE, c8y_event.event_type, c8y_event.text, time
        ))
    }

    fn can_send_over_mqtt(&self, message: &Message) -> bool {
        let Some(max_size) = self.max_mqtt_payload_size else {
            return true;
        };
        message.payload.len() < max_size
    }

    pub fn process_cached_messages(
        &mut self,
        context: &FlowContextHandle,
        message: &Message,
    ) -> Result<Vec<Message>, FlowError> {
        let birth_message = message.payload_str().ok_or_else(|| {
            FlowError::UnsupportedMessage(format!(
                "Not an UTF8 event payload received on: {}",
                message.topic
            ))
        })?;

        let pending_messages = take_cached_telemetry_data(&mut self.cache, birth_message);

        let mut messages = vec![];
        for pending in pending_messages {
            messages.push(self.process_cached_message(context, pending)?);
        }
        Ok(messages)
    }

    pub fn process_cached_message(
        &mut self,
        context: &FlowContextHandle,
        message: Message,
    ) -> Result<Message, FlowError> {
        match self.mqtt_schema.entity_channel_of(&message.topic) {
            Ok((entity_id, Channel::Event { event_type })) => {
                let Some(entity) = get_entity_metadata(context, entity_id.as_str()) else {
                    return Err(FlowError::UnsupportedMessage(format!(
                        "Unknown entity: {entity_id}"
                    )));
                };

                self.convert(entity, &event_type, &message)
            }
            _ => Err(FlowError::UnsupportedMessage(format!(
                "Not a measurement topic: {}",
                message.topic
            ))),
        }
    }
}
