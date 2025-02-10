use async_trait::async_trait;
use tedge_actors::LoggingSender;
use tedge_actors::MessageSink;
use tedge_actors::Sender;
use tedge_actors::Server;
use tedge_api::entity::EntityMetadata;
use tedge_api::entity_store;
use tedge_api::entity_store::EntityRegistrationMessage;
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
}

#[derive(Debug)]
pub enum EntityStoreResponse {
    Get(Option<EntityMetadata>),
    Create(Result<Vec<RegisteredEntityData>, entity_store::Error>),
    Delete(Vec<EntityTopicId>),
    List(Vec<EntityMetadata>),
    Ok,
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
                self.process_entity_data(topic_id).await;
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
            Ok(entity) => {
                if let Err(err) = self.entity_store.update(entity.clone()) {
                    error!(
                        "Failed to register entity registration message: {entity:?} due to {err}"
                    );
                }
            }
            Err(()) => error!("Failed to parse {message} as an entity registration message"),
        }
    }

    async fn process_entity_data(&mut self, topic_id: EntityTopicId) {
        // if the target entity is unregistered, try to register it first using auto-registration
        if self.entity_store.get(&topic_id).is_none()
            && self.entity_auto_register
            && topic_id.matches_default_topic_scheme()
        {
            match self.entity_store.auto_register_entity(&topic_id) {
                Ok(entities) => {
                    for entity in entities {
                        let message = entity.to_mqtt_message(&self.mqtt_schema).with_retain();
                        if let Err(err) = self.mqtt_publisher.send(message).await {
                            error!(
                                "Failed to publish auto-registration messages for the topic: {topic_id} due to {err}",
                            )
                        }
                    }
                }
                Err(err) => {
                    error!(
                        "Failed to auto-register entities for the topic: {} due to: {err}",
                        &topic_id
                    );
                }
            }
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
            if let Err(err) = self.mqtt_publisher.send(message.clone()).await {
                error!(
                    "Failed to publish the entity registration message: {message:?} due to {err}",
                )
            }
        }
        Ok(registered)
    }

    async fn deregister_entity(&mut self, topic_id: EntityTopicId) -> Vec<EntityTopicId> {
        let deleted = self.entity_store.deregister_entity(&topic_id);
        for topic_id in deleted.iter() {
            let topic = self
                .mqtt_schema
                .topic_for(topic_id, &Channel::EntityMetadata);
            let clear_entity_msg = MqttMessage::new(&topic, "")
                .with_retain()
                .with_qos(QoS::AtLeastOnce);

            if let Err(err) = self.mqtt_publisher.send(clear_entity_msg).await {
                error!("Failed to publish clear message for the topic: {topic_id} due to {err}",)
            }
        }

        deleted
    }
}

pub fn subscriptions(topic_root: &str) -> TopicFilter {
    let topic = format!("{}/+/+/+/+/#", topic_root);
    vec![topic].try_into().unwrap()
}
