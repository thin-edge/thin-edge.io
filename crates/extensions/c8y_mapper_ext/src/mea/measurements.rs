use crate::json;
use crate::mea::get_entity_metadata;
use crate::mea::get_measurement_units;
use std::time::SystemTime;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_flows::ConfigError;
use tedge_flows::FlowContextHandle;
use tedge_flows::FlowError;
use tedge_flows::JsonValue;
use tedge_flows::Message;

#[derive(Clone, Default)]
pub struct MeasurementConverter {
    mqtt_schema: MqttSchema,
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
        &self,
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
            Ok((entity, Channel::Measurement { measurement_type })) => {
                self.convert(context, entity.as_str(), &measurement_type, payload)
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
        entity_id: &str,
        measurement_type: &str,
        payload: &str,
    ) -> Result<Vec<Message>, FlowError> {
        let Some(entity) = get_entity_metadata(context, entity_id) else {
            return Ok(vec![]);
        };
        let units =
            get_measurement_units(context, &self.mqtt_schema.root, entity_id, measurement_type);
        let Ok(c8y_json_payload) =
            json::from_thin_edge_json(payload, &entity, measurement_type, units.as_ref())
        else {
            return Err(FlowError::UnsupportedMessage(
                "Not a thin-edge measurement".to_string(),
            ));
        };
        Ok(vec![Message::new(
            "c8y/measurement/measurements/create",
            c8y_json_payload,
        )])
    }
}
