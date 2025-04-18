use async_trait::async_trait;
use serde_json::Map;
use serde_json::Value;
use tedge_actors::LoggingSender;
use tedge_actors::MessageSink;
use tedge_actors::Sender;
use tedge_actors::Server;
use tedge_api::entity::EntityMetadata;
use tedge_api::entity_store;
use tedge_api::entity_store::EntityRegistrationMessage;
use tedge_api::entity_store::EntityTwinMessage;
use tedge_api::entity_store::ListFilters;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::pending_entity_store::RegisteredEntityData;
use tedge_api::EntityStore;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::QoS;
use tedge_mqtt_ext::TopicFilter;
use tracing::error;

#[derive(Debug)]
pub enum EntityStoreRequest {
    Get(EntityTopicId),
    Create(EntityRegistrationMessage),
    Delete(EntityTopicId),
    List(ListFilters),
    MqttMessage(MqttMessage),
    GetTwinFragment(EntityTopicId, String),
    SetTwinFragment(EntityTwinMessage),
    GetTwinFragments(EntityTopicId),
    SetTwinFragments(EntityTopicId, Map<String, Value>),
}

#[derive(Debug)]
pub enum EntityStoreResponse {
    Get(Option<EntityMetadata>),
    Create(Result<Vec<RegisteredEntityData>, entity_store::Error>),
    Delete(Vec<EntityMetadata>),
    List(Vec<EntityMetadata>),
    Ok,
    GetTwinFragment(Option<Value>),
    SetTwinFragment(Result<bool, entity_store::Error>),
    GetTwinFragments(Result<Map<String, Value>, entity_store::Error>),
    SetTwinFragments(Result<(), entity_store::Error>),
}

pub struct EntityStoreServer {
    entity_store: EntityStore,
    mqtt_schema: MqttSchema,
    mqtt_publisher: LoggingSender<MqttMessage>,
    entity_auto_register: bool,
}

impl EntityStoreServer {
    pub fn new(
        entity_store: EntityStore,
        mqtt_schema: MqttSchema,
        mqtt_actor: &mut impl MessageSink<MqttMessage>,
        entity_auto_register: bool,
    ) -> Self {
        let mqtt_publisher = LoggingSender::new("MqttPublisher".into(), mqtt_actor.get_sender());

        Self {
            entity_store,
            mqtt_schema,
            mqtt_publisher,
            entity_auto_register,
        }
    }

    #[cfg(test)]
    pub fn entity_topic_ids(&self) -> impl Iterator<Item = &EntityTopicId> {
        self.entity_store.entity_topic_ids()
    }

    #[cfg(test)]
    pub fn get(&self, entity_topic_id: &EntityTopicId) -> Option<&EntityMetadata> {
        self.entity_store.get(entity_topic_id)
    }
}

#[async_trait]
impl Server for EntityStoreServer {
    type Request = EntityStoreRequest;
    type Response = EntityStoreResponse;

    fn name(&self) -> &str {
        "EntityStoreServer"
    }

    async fn handle(&mut self, request: EntityStoreRequest) -> EntityStoreResponse {
        match request {
            EntityStoreRequest::Get(topic_id) => {
                let entity = self.entity_store.get(&topic_id);
                EntityStoreResponse::Get(entity.cloned())
            }
            EntityStoreRequest::Create(entity) => {
                let res = self.register_entity(entity).await;
                EntityStoreResponse::Create(res)
            }
            EntityStoreRequest::Delete(topic_id) => {
                let deleted_entities = self.deregister_entity(topic_id).await;
                EntityStoreResponse::Delete(deleted_entities)
            }
            EntityStoreRequest::List(filters) => {
                let entities = self.entity_store.list_entity_tree(filters);
                EntityStoreResponse::List(entities.into_iter().cloned().collect())
            }
            EntityStoreRequest::GetTwinFragment(topic_id, fragment_key) => {
                let twin = self
                    .entity_store
                    .get_twin_fragment(&topic_id, &fragment_key);
                EntityStoreResponse::GetTwinFragment(twin.cloned())
            }
            EntityStoreRequest::SetTwinFragment(twin_data) => {
                let res = self.set_twin_fragment(twin_data).await;
                EntityStoreResponse::SetTwinFragment(res)
            }
            EntityStoreRequest::GetTwinFragments(topic_id) => {
                let res = self.entity_store.get_twin_fragments(&topic_id);
                EntityStoreResponse::GetTwinFragments(res.cloned())
            }
            EntityStoreRequest::SetTwinFragments(topic_id, fragments) => {
                let res = self.set_entity_twin_fragments(&topic_id, fragments).await;
                EntityStoreResponse::SetTwinFragments(res)
            }
            EntityStoreRequest::MqttMessage(mqtt_message) => {
                self.process_mqtt_message(mqtt_message).await;
                EntityStoreResponse::Ok
            }
        }
    }
}

impl EntityStoreServer {
    async fn process_mqtt_message(&mut self, message: MqttMessage) {
        if let Ok((topic_id, channel)) = self.mqtt_schema.entity_channel_of(&message.topic) {
            if let Channel::EntityMetadata = channel {
                self.process_entity_registration(topic_id, message).await;
            } else {
                let res = self.process_entity_data(topic_id, channel, message).await;
                if let Err(err) = res {
                    error!("Failed to process entity data message: {err}");
                }
            }
        } else {
            error!("Ignoring the message: {message} received on unsupported topic",);
        }
    }

    async fn process_entity_registration(&mut self, topic_id: EntityTopicId, message: MqttMessage) {
        if message.payload().is_empty() {
            let _ = self.deregister_entity(topic_id).await;
            return;
        }

        match EntityRegistrationMessage::try_from(&message) {
            Ok(entity) => match self.entity_store.update(entity.clone()) {
                Ok(registered) => {
                    for entity in registered {
                        for (fragment_key, fragment_value) in entity.reg_message.twin_data {
                            self.publish_twin_data(
                                &entity.reg_message.topic_id,
                                fragment_key,
                                fragment_value,
                            )
                            .await;
                        }
                    }
                }
                Err(err) => {
                    error!(
                        "Failed to register entity registration message: {entity:?} due to {err}"
                    );
                }
            },
            Err(()) => error!("Failed to parse {message} as an entity registration message"),
        }
    }

    async fn process_entity_data(
        &mut self,
        topic_id: EntityTopicId,
        channel: Channel,
        message: MqttMessage,
    ) -> Result<(), entity_store::Error> {
        // if the target entity is unregistered, try to register it first using auto-registration
        if self.entity_store.get(&topic_id).is_none()
            && self.entity_auto_register
            && topic_id.matches_default_topic_scheme()
            && !message.payload().is_empty()
        {
            let entities = self.entity_store.auto_register_entity(&topic_id)?;
            for entity in entities {
                let message = entity.to_mqtt_message(&self.mqtt_schema).with_retain();
                self.publish_message(message).await;
            }
        }

        if let Channel::EntityTwinData { fragment_key } = channel {
            let fragment_value = serde_json::from_slice(message.payload_bytes())?;
            let twin_message = EntityTwinMessage::new(topic_id, fragment_key, fragment_value);
            self.entity_store.update_twin_fragment(twin_message)?;
        }

        Ok(())
    }

    async fn set_twin_fragment(
        &mut self,
        twin_message: EntityTwinMessage,
    ) -> Result<bool, entity_store::Error> {
        let updated = self
            .entity_store
            .update_twin_fragment(twin_message.clone())?;
        if updated {
            self.publish_twin_data(
                &twin_message.topic_id,
                twin_message.fragment_key,
                twin_message.fragment_value,
            )
            .await;
        }

        Ok(updated)
    }

    async fn publish_twin_data(
        &mut self,
        topic_id: &EntityTopicId,
        fragment_key: String,
        fragment_value: Value,
    ) {
        let twin_channel = Channel::EntityTwinData { fragment_key };
        let topic = self.mqtt_schema.topic_for(topic_id, &twin_channel);
        let payload = if fragment_value.is_null() {
            "".to_string()
        } else {
            fragment_value.to_string()
        };
        let message = MqttMessage::new(&topic, payload).with_retain();
        self.publish_message(message).await;
    }

    async fn publish_message(&mut self, message: MqttMessage) {
        let topic = message.topic.clone();
        if let Err(err) = self.mqtt_publisher.send(message).await {
            error!("Failed to publish the message on topic: {topic:?} due to {err}");
        }
    }

    async fn register_entity(
        &mut self,
        entity: EntityRegistrationMessage,
    ) -> Result<Vec<RegisteredEntityData>, entity_store::Error> {
        if self.entity_store.get(&entity.topic_id).is_some() {
            return Err(entity_store::Error::EntityAlreadyRegistered(
                entity.topic_id,
            ));
        }

        if let Some(parent) = entity.parent.as_ref() {
            if self.entity_store.get(parent).is_none() {
                return Err(entity_store::Error::NoParent(
                    parent.to_string().into_boxed_str(),
                ));
            }
        }

        let registered = self.entity_store.update(entity.clone())?;

        if !registered.is_empty() {
            let message = entity.to_mqtt_message(&self.mqtt_schema);
            self.publish_message(message).await;
        }
        Ok(registered)
    }

    async fn deregister_entity(&mut self, topic_id: EntityTopicId) -> Vec<EntityMetadata> {
        let deleted = self.entity_store.deregister_entity(&topic_id);
        for entity in deleted.iter().rev() {
            for twin_key in entity.twin_data.keys() {
                let topic = self.mqtt_schema.topic_for(
                    &entity.topic_id,
                    &Channel::EntityTwinData {
                        fragment_key: twin_key.to_string(),
                    },
                );
                let clear_twin_msg = MqttMessage::new(&topic, "").with_retain();

                self.publish_message(clear_twin_msg).await;
            }
            let topic = self
                .mqtt_schema
                .topic_for(&entity.topic_id, &Channel::EntityMetadata);
            let clear_entity_msg = MqttMessage::new(&topic, "")
                .with_retain()
                .with_qos(QoS::AtLeastOnce);

            self.publish_message(clear_entity_msg).await;
        }

        deleted
    }

    async fn set_entity_twin_fragments(
        &mut self,
        topic_id: &EntityTopicId,
        fragments: Map<String, Value>,
    ) -> Result<(), entity_store::Error> {
        let mut old_fragments = self
            .entity_store
            .set_twin_fragments(topic_id, fragments.clone())?;

        let fragments_to_clear = old_fragments
            .keys()
            .filter(|key| !fragments.contains_key(*key))
            .cloned()
            .collect::<Vec<_>>();

        // Clear all old twin messages
        for fragment_key in fragments_to_clear.into_iter() {
            let twin_message = EntityTwinMessage::new(topic_id.clone(), fragment_key, Value::Null);
            let message = twin_message.to_mqtt_message(&self.mqtt_schema);
            self.publish_message(message).await;
        }

        // Publish new twin messages
        for (fragment_key, fragment_value) in fragments.into_iter() {
            let old_value = old_fragments.remove(&fragment_key);
            if old_value == Some(fragment_value.clone()) {
                continue;
            }
            let twin_message =
                EntityTwinMessage::new(topic_id.clone(), fragment_key, fragment_value);

            let message = twin_message.to_mqtt_message(&self.mqtt_schema);
            self.publish_message(message).await;
        }

        Ok(())
    }
}

pub fn subscriptions(topic_root: &str) -> TopicFilter {
    let topic = format!("{}/+/+/+/+/#", topic_root);
    vec![topic].try_into().unwrap()
}
