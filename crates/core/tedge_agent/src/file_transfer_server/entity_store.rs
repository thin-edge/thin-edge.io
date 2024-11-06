use axum::extract::Path;
use axum::extract::State;
use axum::response::IntoResponse;
use axum::response::Response;
use axum::routing::post;
use axum::Json;
use axum::Router;
use hyper::StatusCode;
use serde::Deserialize;
use serde::Serialize;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::Mutex;
use tedge_api::entity_store;
use tedge_api::entity_store::EntityMetadata;
use tedge_api::entity_store::EntityRegistrationMessage;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::TopicIdError;
use tedge_api::EntityStore;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct EntityDef {
    topic_id: String,
}

#[derive(thiserror::Error, Debug)]
enum Error {
    #[error(transparent)]
    InvalidEntityTopicId(#[from] TopicIdError),

    #[allow(clippy::enum_variant_names)]
    #[error(transparent)]
    EntityStoreError(#[from] entity_store::Error),

    #[error("Entity not found with topic id: {0}")]
    EntityNotFound(EntityTopicId),
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let status_code = match &self {
            Error::InvalidEntityTopicId(_) => StatusCode::BAD_REQUEST,
            Error::EntityStoreError(_) => StatusCode::BAD_REQUEST,
            Error::EntityNotFound(_) => StatusCode::NOT_FOUND,
        };
        let error_message = self.to_string();

        (status_code, error_message).into_response()
    }
}

pub(crate) fn entity_store_router(state: Arc<Mutex<EntityStore>>) -> Router {
    Router::new()
        .route(
            "/v1/entities/*path",
            post(register_entity).get(get_entity), // .delete(deregister_entity),
        )
        .with_state(state)
}

async fn register_entity(
    State(state): State<Arc<Mutex<EntityStore>>>,
    Json(entity): Json<EntityRegistrationMessage>,
) -> Result<StatusCode, Error> {
    let mut entity_store = state.lock().unwrap();

    entity_store.update(entity)?;
    Ok(StatusCode::OK)
}

async fn get_entity(
    State(state): State<Arc<Mutex<EntityStore>>>,
    Path(path): Path<String>,
) -> Result<Json<EntityMetadata>, Error> {
    let entity_store = state.lock().unwrap();
    let topic_id = EntityTopicId::from_str(&path)?;

    if let Some(entity) = entity_store.get(&topic_id) {
        Ok(Json(entity.clone()))
    } else {
        Err(Error::EntityNotFound(topic_id))
    }
}

#[cfg(test)]
mod tests {
    use crate::file_transfer_server::entity_store::entity_store_router;
    use hyper::Body;
    use hyper::Method;
    use hyper::Request;
    use hyper::StatusCode;
    use serde_json::Map;
    use std::collections::HashSet;
    use std::sync::Arc;
    use std::sync::Mutex;
    use tedge_api::entity_store::EntityExternalId;
    use tedge_api::entity_store::EntityMetadata;
    use tedge_api::entity_store::EntityRegistrationMessage;
    use tedge_api::entity_store::EntityType;
    use tedge_api::entity_store::InvalidExternalIdError;
    use tedge_api::mqtt_topics::EntityTopicId;
    use tedge_api::mqtt_topics::MqttSchema;
    use tedge_api::EntityStore;
    use tedge_test_utils::fs::TempTedgeDir;
    use tower::Service;

    #[tokio::test]
    async fn entity_get() {
        let ttd = TempTedgeDir::new();

        let mut entity_store = new_entity_store(&ttd, true);
        let _ = entity_store
            .update(EntityRegistrationMessage {
                topic_id: EntityTopicId::default_child_device("test-child").unwrap(),
                external_id: Some("test-child".into()),
                r#type: EntityType::ChildDevice,
                parent: None,
                other: Map::new(),
            })
            .unwrap();
        let state = Arc::new(Mutex::new(entity_store));
        let mut app = entity_store_router(state);

        let topic_id = "device/test-child//";
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/entities/{topic_id}"))
            .body(Body::empty())
            .expect("request builder");

        let response = app.call(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
        let entity: EntityMetadata = serde_json::from_slice(&body).unwrap();
        assert_eq!(entity.topic_id.as_str(), topic_id);
        assert_eq!(entity.r#type, EntityType::ChildDevice);
    }

    #[tokio::test]
    async fn entity_put() {
        let ttd = TempTedgeDir::new();

        let entity_store = new_entity_store(&ttd, true);
        let state = Arc::new(Mutex::new(entity_store));
        let mut app = entity_store_router(state.clone());

        let entity = EntityRegistrationMessage {
            topic_id: EntityTopicId::default_child_device("test-child").unwrap(),
            external_id: Some("test-child".into()),
            r#type: EntityType::ChildDevice,
            parent: None,
            other: Map::new(),
        };
        let payload = serde_json::to_string(&entity).unwrap();

        let topic_id = "device/test-child//";
        let req = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/entities/{topic_id}"))
            .header("Content-Type", "application/json")
            .body(Body::from(payload))
            .expect("request builder");

        let response = app.call(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let entity_store = state.lock().unwrap();
        let entity = entity_store
            .get(&EntityTopicId::default_child_device("test-child").unwrap())
            .unwrap();

        assert_eq!(entity.topic_id.as_str(), topic_id);
        assert_eq!(entity.r#type, EntityType::ChildDevice);
    }

    pub fn new_entity_store(temp_dir: &TempTedgeDir, clean_start: bool) -> EntityStore {
        EntityStore::with_main_device_and_default_service_type(
            MqttSchema::default(),
            EntityRegistrationMessage {
                topic_id: EntityTopicId::default_main_device(),
                external_id: Some("test-device".into()),
                r#type: EntityType::MainDevice,
                parent: None,
                other: Map::new(),
            },
            "service".into(),
            dummy_external_id_mapper,
            dummy_external_id_validator,
            5,
            temp_dir.path(),
            clean_start,
        )
        .unwrap()
    }

    fn dummy_external_id_mapper(
        entity_topic_id: &EntityTopicId,
        _main_device_xid: &EntityExternalId,
    ) -> EntityExternalId {
        entity_topic_id
            .to_string()
            .trim_end_matches('/')
            .replace('/', ":")
            .into()
    }

    fn dummy_external_id_validator(id: &str) -> Result<EntityExternalId, InvalidExternalIdError> {
        let forbidden_chars = HashSet::from(['/', '+', '#']);
        for c in id.chars() {
            if forbidden_chars.contains(&c) {
                return Err(InvalidExternalIdError {
                    external_id: id.into(),
                    invalid_char: c,
                });
            }
        }
        Ok(id.into())
    }
}
