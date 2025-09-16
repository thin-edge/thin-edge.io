use crate::converter::CumulocityConverter;
use crate::error::ConversionError;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::SignalType;
use tedge_mqtt_ext::MqttMessage;

impl CumulocityConverter {
    pub fn process_signal_message(
        &mut self,
        source: &EntityTopicId,
        signal_type: &SignalType,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        let mut messages = Vec::new();

        if source.default_service_name() != Some("tedge-mapper-c8y") {
            return Ok(messages);
        }

        match signal_type {
            SignalType::Sync => {
                for external_id in self.entity_cache.get_all_external_ids() {
                    if let Ok(message) =
                        self.load_and_create_supported_operations_messages(external_id.as_ref())
                    {
                        messages.push(message);
                    }
                }
            }
            SignalType::Custom(_) => {}
        }

        Ok(messages)
    }
}
