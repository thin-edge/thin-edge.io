use crate::entity_cache::CloudEntityMetadata;
use crate::mea::get_entity_metadata;
use crate::mea::get_entity_parent_metadata;
use crate::service_monitor::convert_health_status_message;
use std::str::FromStr;
use std::time::SystemTime;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_config::models::TopicPrefix;
use tedge_flows::ConfigError;
use tedge_flows::FlowContextHandle;
use tedge_flows::FlowError;
use tedge_flows::JsonValue;
use tedge_flows::Message;

#[derive(Clone)]
pub struct HealthStatusConverter {
    mqtt_schema: MqttSchema,
    main_device: EntityTopicId,
    c8y_prefix: TopicPrefix,
}

impl Default for HealthStatusConverter {
    fn default() -> Self {
        HealthStatusConverter {
            mqtt_schema: MqttSchema::default(),
            main_device: EntityTopicId::default_main_device(),
            c8y_prefix: TopicPrefix::try_new("c8y").unwrap(),
        }
    }
}

impl tedge_flows::Transformer for HealthStatusConverter {
    fn name(&self) -> &str {
        "into_c8y_health_status"
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
        if let Some(main_device) = config.string_property("main_device") {
            self.main_device = EntityTopicId::from_str(main_device).map_err(|err| {
                ConfigError::IncorrectSetting(format!("Not a valid entity topic id: {}", err))
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
        match self.mqtt_schema.entity_channel_of(&message.topic) {
            Ok((entity_id, Channel::Health)) => {
                let Some(entity) = get_entity_metadata(context, entity_id.as_str()) else {
                    return Ok(vec![]);
                };
                self.convert(context, entity, message)
            }

            _ => Err(FlowError::UnsupportedMessage(format!(
                "Not a health status topic: {}",
                message.topic
            ))),
        }
    }
}

impl HealthStatusConverter {
    pub fn convert(
        &self,
        context: &FlowContextHandle,
        entity: CloudEntityMetadata,
        message: &Message,
    ) -> Result<Vec<Message>, FlowError> {
        let Some(main_xid) =
            get_entity_metadata(context, self.main_device.as_str()).map(|main| main.external_id)
        else {
            return Ok(vec![]);
        };

        let parent_xid =
            get_entity_parent_metadata(context, &entity).map(|parent| parent.external_id);

        let mqtt_input = message.clone().try_into().map_err(|err| {
            FlowError::UnsupportedMessage(format!("Invalid health status: {err}"))
        })?;

        let mqtt_output = convert_health_status_message(
            &self.mqtt_schema,
            &entity,
            parent_xid.as_ref(),
            &main_xid,
            &mqtt_input,
            &self.c8y_prefix,
        );
        Ok(mqtt_output.into_iter().map(Message::from).collect())
    }
}
