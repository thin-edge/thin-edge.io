use async_trait::async_trait;
use tedge_actors::LoggingSender;
use tedge_actors::MessageSink;
use tedge_actors::Sender;
use tedge_actors::Server;
use tedge_api::entity::EntityMetadata;
use tedge_api::entity_store;
use tedge_api::entity_store::EntityRegistrationMessage;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::pending_entity_store::PendingEntityData;
use tedge_api::EntityStore;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::TopicFilter;
use tracing::error;

#[derive(Debug)]
pub enum EntityStoreRequest {
    Get(EntityTopicId),
    Create(EntityRegistrationMessage),
    Delete(EntityTopicId),
    MqttMessage(MqttMessage),
}

#[derive(Debug)]
pub enum EntityStoreResponse {
    Get(Option<EntityMetadata>),
    Create(Result<(Vec<EntityTopicId>, Vec<PendingEntityData>), entity_store::Error>),
    Delete(Vec<EntityTopicId>),
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
                let res = self.entity_store.update(entity);
                EntityStoreResponse::Create(res)
            }
            EntityStoreRequest::Delete(topic_id) => {
                let deleted_entities = self.entity_store.deregister_entity(&topic_id);
                EntityStoreResponse::Delete(deleted_entities)
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
            self.process_entity_registration(message);
        } else {
            self.process_entity_data(topic_id).await;
        }
        } else {
            error!("Ignoring the message: {message} received on unsupported topic",);
        }
    }

    fn process_entity_registration(&mut self, message: MqttMessage) {
        if message.payload().is_empty() {
            // Nothing to do on entity clear messages
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
}

pub fn subscriptions(topic_root: &str) -> TopicFilter {
    let topic = format!("{}/+/+/+/+/#", topic_root);
    vec![topic].try_into().unwrap()
}
