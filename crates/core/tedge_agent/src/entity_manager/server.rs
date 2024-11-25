use async_trait::async_trait;
use tedge_actors::Server;
use tedge_api::entity_store;
use tedge_api::entity_store::EntityMetadata;
use tedge_api::entity_store::EntityRegistrationMessage;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::pending_entity_store::PendingEntityData;
use tedge_api::EntityStore;
use tedge_mqtt_ext::TopicFilter;

#[derive(Debug)]
pub enum EntityStoreRequest {
    Get(EntityTopicId),
    Create(EntityRegistrationMessage),
    Delete(EntityTopicId),
}

#[derive(Debug)]
pub enum EntityStoreResponse {
    Get(Option<EntityMetadata>),
    Create(Result<(Vec<EntityTopicId>, Vec<PendingEntityData>), entity_store::Error>),
    Delete(Vec<EntityTopicId>),
}

pub struct EntityStoreServer {
    entity_store: EntityStore,
}

impl EntityStoreServer {
    pub fn new(entity_store: EntityStore) -> Self {
        Self { entity_store }
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
        }
    }
}

pub fn subscriptions(topic_root: &str) -> TopicFilter {
    let topic = format!("{}/+/+/+/+/#", topic_root);
    vec![topic].try_into().unwrap()
}
