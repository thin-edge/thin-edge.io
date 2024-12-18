use async_trait::async_trait;
use tedge_actors::LoggingSender;
use tedge_actors::MessageSink;
use tedge_actors::Sender;
use tedge_actors::Server;
use tedge_api::entity_store;
use tedge_api::entity_store::EntityMetadata;
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
}

impl EntityStoreServer {
    pub fn new(
        entity_store: EntityStore,
        mqtt_schema: MqttSchema,
        mqtt_actor: &mut impl MessageSink<MqttMessage>,
    ) -> Self {
        let mqtt_publisher = LoggingSender::new("MqttPublisher".into(), mqtt_actor.get_sender());

        Self {
            entity_store,
            mqtt_schema,
            mqtt_publisher,
        }
    }
}

#[async_trait]
impl Server for EntityStoreServer {
    type Request = EntityStoreRequest;
    type Response = EntityStoreResponse;

    fn name(&self) -> &str {
        "ConcurrentWorker"
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
        let (topic_id, channel) = self
            .mqtt_schema
            .entity_channel_of(&message.topic)
            .expect("Topic schema must have been pre-validated");
        if let Channel::EntityMetadata = channel {
            self.process_entity_registration(message);
        } else {
            self.process_entity_data(topic_id).await;
        }
    }

    fn process_entity_registration(&mut self, message: MqttMessage) {
        match EntityRegistrationMessage::try_from(&message) {
            Ok(entity) => match self.entity_store.update(entity.clone()) {
                Ok((_, pending_entities)) => {
                    for pending_entity in pending_entities {
                        if let Err(err) =
                            self.entity_store.update(pending_entity.reg_message.clone())
                        {
                            error!(
                                "Failed to register pending entity: {:?} for root entity {:?} due to {err}",
                                &pending_entity.reg_message, &entity
                            )
                        }
                    }
                }
                Err(err) => error!(
                    "Failed to register entity registration message: {} due to {err}",
                    &message
                ),
            },
            Err(_) => error!("Failed to update entity store with {}", &message),
        }
    }

    async fn process_entity_data(&mut self, topic_id: EntityTopicId) {
        // if the target entity is unregistered, try to register it first using auto-registration
        if self.entity_store.get(&topic_id).is_none()
            // && self.config.enable_auto_register
            && topic_id.matches_default_topic_scheme()
        {
            match self.entity_store.auto_register_entity(&topic_id) {
                Ok(entities) => {
                    for entity in entities {
                        let message = entity.to_mqtt_message(&self.mqtt_schema);
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

pub fn subscriptions() -> TopicFilter {
    vec!["te/+/+/+/+/#"].try_into().unwrap()
}
