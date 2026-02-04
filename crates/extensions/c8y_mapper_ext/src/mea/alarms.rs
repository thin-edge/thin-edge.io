use crate::entity_cache::CloudEntityMetadata;
use crate::mea::get_entity_metadata;
use std::time::SystemTime;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_config::models::TopicPrefix;
use tedge_flows::ConfigError;
use tedge_flows::FlowContextHandle;
use tedge_flows::FlowError;
use tedge_flows::JsonValue;
use tedge_flows::Message;
use tedge_mqtt_ext::MqttMessage;

#[derive(Clone)]
pub struct AlarmConverter {
    mqtt_schema: MqttSchema,
    c8y_prefix: TopicPrefix,
    state: crate::alarm_converter::AlarmConverter,
}

impl Default for AlarmConverter {
    fn default() -> Self {
        AlarmConverter {
            mqtt_schema: MqttSchema::default(),
            c8y_prefix: TopicPrefix::try_new("c8y").unwrap(),
            state: crate::alarm_converter::AlarmConverter::new(),
        }
    }
}

impl tedge_flows::Transformer for AlarmConverter {
    fn name(&self) -> &str {
        "into_c8y_alarms"
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
        Ok(())
    }

    fn on_message(
        &mut self,
        _timestamp: SystemTime,
        message: &Message,
        context: &FlowContextHandle,
    ) -> Result<Vec<Message>, FlowError> {
        if message
            .topic
            .starts_with(crate::alarm_converter::INTERNAL_ALARMS_TOPIC)
        {
            self.process_internal_alarm(message.clone())?;
            return Ok(vec![]);
        };
        match self.mqtt_schema.entity_channel_of(&message.topic) {
            Ok((entity_id, Channel::Alarm { alarm_type })) => {
                let Some(entity) = get_entity_metadata(context, entity_id.as_str()) else {
                    return Ok(vec![]);
                };
                self.convert(entity, &alarm_type, message.clone())
            }

            _ => Err(FlowError::UnsupportedMessage(format!(
                "Not an alarm topic: {}",
                message.topic
            ))),
        }
    }

    fn is_periodic(&self) -> bool {
        true
    }

    fn on_interval(
        &mut self,
        timestamp: SystemTime,
        context: &FlowContextHandle,
    ) -> Result<Vec<Message>, FlowError> {
        let sync_messages: Vec<MqttMessage> = self.state.sync();
        self.state = crate::alarm_converter::AlarmConverter::Synced;

        let mut c8y_alarms = vec![];
        for message in sync_messages {
            let alarm = message.into();
            let c8y_alarm = self.on_message(timestamp, &alarm, context)?;
            c8y_alarms.extend(c8y_alarm);
        }
        Ok(c8y_alarms)
    }
}

impl AlarmConverter {
    fn process_internal_alarm(&mut self, message: Message) -> Result<(), FlowError> {
        let alarm = message.try_into().map_err(|err| {
            FlowError::UnsupportedMessage(format!("Not an internal alarm: {}", err))
        })?;
        self.state.process_internal_alarm(&alarm);
        Ok(())
    }

    fn convert(
        &mut self,
        entity: CloudEntityMetadata,
        alarm_type: &str,
        alarm: Message,
    ) -> Result<Vec<Message>, FlowError> {
        let alarm = alarm
            .try_into()
            .map_err(|err| FlowError::UnsupportedMessage(format!("Not an alarm: {}", err)))?;
        let mqtt_messages = self
            .state
            .try_convert_alarm(
                entity.topic_id(),
                &entity.external_id,
                &entity.r#type(),
                &alarm,
                alarm_type,
                &self.c8y_prefix,
            )
            .map_err(|err| {
                FlowError::UnsupportedMessage(format!("Alarm conversion error: {}", err))
            })?;

        Ok(mqtt_messages.into_iter().map(Message::from).collect())
    }
}
