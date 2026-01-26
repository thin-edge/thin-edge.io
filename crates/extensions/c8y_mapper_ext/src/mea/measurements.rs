use crate::entity_cache::CloudEntityMetadata;
use crate::json;
use crate::mea::get_entity_metadata;
use crate::mea::get_measurement_units;
use crate::mea::take_cached_telemetry_data;
use std::time::SystemTime;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::store::RingBuffer;
use tedge_flows::ConfigError;
use tedge_flows::FlowContextHandle;
use tedge_flows::FlowError;
use tedge_flows::JsonValue;
use tedge_flows::Message;

#[derive(Clone, Default)]
pub struct MeasurementConverter {
    mqtt_schema: MqttSchema,
    cache: RingBuffer<Message>,
}

impl tedge_flows::Transformer for MeasurementConverter {
    fn name(&self) -> &str {
        "into_c8y_measurements"
    }

    fn set_config(&mut self, config: JsonValue) -> Result<(), ConfigError> {
        if let Some(root) = config.string_property("topic_root") {
            self.mqtt_schema = MqttSchema::with_root(root.to_string())
        }
        Ok(())
    }

    fn on_message(
        &mut self,
        _timestamp: SystemTime,
        message: &Message,
        context: &FlowContextHandle,
    ) -> Result<Vec<Message>, FlowError> {
        let Some(payload) = message.payload_str() else {
            return Err(FlowError::UnsupportedMessage(
                "Not an UTF8 payload".to_string(),
            ));
        };
        match self.mqtt_schema.entity_channel_of(&message.topic) {
            Ok((entity_id, Channel::Measurement { measurement_type })) => {
                let Some(entity) = get_entity_metadata(context, entity_id.as_str()) else {
                    self.cache.push(message.clone());
                    return Ok(vec![]);
                };

                let measurement = self.convert(context, entity, &measurement_type, payload)?;
                Ok(vec![measurement])
            }
            Ok((_, Channel::Status { component })) if component == "entities" => {
                self.process_cached_messages(context, payload)
            }
            _ => Err(FlowError::UnsupportedMessage(format!(
                "Not a measurement topic: {}",
                message.topic
            ))),
        }
    }
}

impl MeasurementConverter {
    pub fn convert(
        &self,
        context: &FlowContextHandle,
        entity: CloudEntityMetadata,
        measurement_type: &str,
        payload: &str,
    ) -> Result<Message, FlowError> {
        let units = get_measurement_units(
            context,
            &self.mqtt_schema.root,
            entity.metadata.topic_id.as_str(),
            measurement_type,
        );
        let Ok(c8y_json_payload) =
            json::from_thin_edge_json(payload, &entity, measurement_type, units.as_ref())
        else {
            return Err(FlowError::UnsupportedMessage(
                "Not a thin-edge measurement".to_string(),
            ));
        };
        Ok(Message::new(
            "c8y/measurement/measurements/create",
            c8y_json_payload,
        ))
    }

    pub fn process_cached_messages(
        &mut self,
        context: &FlowContextHandle,
        birth_message: &str,
    ) -> Result<Vec<Message>, FlowError> {
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
        let Some(payload) = message.payload_str() else {
            return Err(FlowError::UnsupportedMessage(
                "Not an UTF8 payload".to_string(),
            ));
        };

        match self.mqtt_schema.entity_channel_of(&message.topic) {
            Ok((entity_id, Channel::Measurement { measurement_type })) => {
                let Some(entity) = get_entity_metadata(context, entity_id.as_str()) else {
                    return Err(FlowError::UnsupportedMessage(format!(
                        "Unknown entity: {entity_id}"
                    )));
                };

                self.convert(context, entity, &measurement_type, payload)
            }
            _ => Err(FlowError::UnsupportedMessage(format!(
                "Not a measurement topic: {}",
                message.topic
            ))),
        }
    }
}
