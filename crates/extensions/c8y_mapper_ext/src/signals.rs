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
            SignalType::Operations => {
                let main_message = self.load_and_create_supported_operations_messages(
                    &self.config.device_id.clone(),
                )?;
                let mut child_messages = self.send_child_supported_operation_messages()?;
                messages.append(&mut vec![main_message]);
                messages.append(&mut child_messages);
            }
            SignalType::Custom(_) => {}
        }

        Ok(messages)
    }

    fn send_child_supported_operation_messages(
        &mut self,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        let mut messages = Vec::new();

        let mut child_supported_operations_messages: Vec<MqttMessage> = Vec::new();
        for child_xid in self.supported_operations.get_child_xids() {
            let message = self.load_and_create_supported_operations_messages(&child_xid)?;
            child_supported_operations_messages.push(message);
        }
        messages.append(&mut child_supported_operations_messages);

        Ok(messages)
    }
}
