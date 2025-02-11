//! This module defines the axum routes and handlers for the entity store REST APIs.
//! The following endpoints are currently supported:
//!
//! - `POST /v1/entities`: Registers a new entity.
//! - `GET /v1/entities/*path`: Retrieves an existing entity.
//! - `DELETE /v1/entities/*path`: Deregisters an existing entity.
//!
//! References:
//!
//! - https://github.com/thin-edge/thin-edge.io/blob/main/design/decisions/0005-entity-registration-api.md
use super::server::AgentState;
use crate::entity_manager::server::EntityStoreRequest;
use crate::entity_manager::server::EntityStoreResponse;
use axum::extract::Path;
use axum::extract::Query;
use axum::extract::State;
use axum::response::IntoResponse;
use axum::response::Response;
use axum::routing::get;
use axum::routing::post;
use axum::Json;
use axum::Router;
use hyper::StatusCode;
use serde::Deserialize;
use serde_json::json;
use std::str::FromStr;
use tedge_api::entity::EntityMetadata;
use tedge_api::entity::InvalidEntityType;
use tedge_api::entity_store;
use tedge_api::entity_store::EntityRegistrationMessage;
use tedge_api::entity_store::ListFilters;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::TopicIdError;

#[derive(Debug, Default, Deserialize)]
pub struct ListParams {
    #[serde(default)]
    root: Option<String>,
    #[serde(default)]
    parent: Option<String>,
    #[serde(default)]
    r#type: Option<String>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq, Clone)]
pub enum InputValidationError {
    #[error(transparent)]
    InvalidEntityType(#[from] InvalidEntityType),
    #[error(transparent)]
    InvalidEntityTopic(#[from] TopicIdError),
    #[error("The provided parameters: {0} and {1} are mutually exclusive. Use either one.")]
    IncompatibleParams(String, String),
}

impl TryFrom<ListParams> for ListFilters {
    type Error = InputValidationError;

    fn try_from(params: ListParams) -> Result<Self, Self::Error> {
        let root = params
            .root
            .filter(|v| !v.is_empty())
            .map(|val| val.parse())
            .transpose()?;
        let parent = params
            .parent
            .filter(|v| !v.is_empty())
            .map(|val| val.parse())
            .transpose()?;
        let r#type = params
            .r#type
            .filter(|v| !v.is_empty())
            .map(|val| val.parse())
            .transpose()?;

        if root.is_some() && parent.is_some() {
            return Err(InputValidationError::IncompatibleParams(
                "root".to_string(),
                "parent".to_string(),
            ));
        }

        Ok(Self {
            root,
            parent,
            r#type,
        })
    }
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

    #[allow(clippy::enum_variant_names)]
    #[error("Failed to publish entity registration message via MQTT")]
    ChannelError(#[from] tedge_actors::ChannelError),

    #[error("Received unexpected response from entity store")]
    InvalidEntityStoreResponse,

    #[error(transparent)]
    InvalidInput(#[from] InputValidationError),
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let status_code = match &self {
            Error::InvalidEntityTopicId(_) => StatusCode::BAD_REQUEST,
            Error::EntityStoreError(err) => match err {
                entity_store::Error::EntityAlreadyRegistered(_) => StatusCode::CONFLICT,
                entity_store::Error::UnknownEntity(_) => StatusCode::NOT_FOUND,
                _ => StatusCode::BAD_REQUEST,
            },
            Error::EntityNotFound(_) => StatusCode::NOT_FOUND,
            Error::ChannelError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Error::InvalidEntityStoreResponse => StatusCode::INTERNAL_SERVER_ERROR,
            Error::InvalidInput(_) => StatusCode::BAD_REQUEST,
        };
        let error_message = self.to_string();

        (status_code, Json(json!({ "error": error_message }))).into_response()
    }
}

pub(crate) fn entity_store_router(state: AgentState) -> Router {
    Router::new()
        .route("/v1/entities", post(register_entity).get(list_entities))
        .route(
            "/v1/entities/{*path}",
            get(get_entity).delete(deregister_entity),
        )
        .with_state(state)
}

async fn register_entity(
    State(state): State<AgentState>,
    Json(entity): Json<EntityRegistrationMessage>,
) -> impl IntoResponse {
    let response = state
        .entity_store_handle
        .clone()
        .await_response(EntityStoreRequest::Create(entity.clone()))
        .await?;
    let EntityStoreResponse::Create(res) = response else {
        return Err(Error::InvalidEntityStoreResponse);
    };

    res?;
    Ok((
        StatusCode::CREATED,
        Json(json!({"@topic-id": entity.topic_id.as_str()})),
    ))
}

async fn get_entity(
    State(state): State<AgentState>,
    Path(path): Path<String>,
) -> Result<Json<EntityMetadata>, Error> {
    let topic_id = EntityTopicId::from_str(&path)?;

    let response = state
        .entity_store_handle
        .clone()
        .await_response(EntityStoreRequest::Get(topic_id.clone()))
        .await?;

    let EntityStoreResponse::Get(entity_metadata) = response else {
        return Err(Error::InvalidEntityStoreResponse);
    };

    if let Some(entity) = entity_metadata {
        Ok(Json(entity.clone()))
    } else {
        Err(Error::EntityNotFound(topic_id))
    }
}

async fn deregister_entity(
    State(state): State<AgentState>,
    Path(path): Path<String>,
) -> Result<Response, Error> {
    let topic_id = EntityTopicId::from_str(&path)?;

    let response = state
        .entity_store_handle
        .clone()
        .await_response(EntityStoreRequest::Delete(topic_id.clone()))
        .await?;

    let EntityStoreResponse::Delete(deleted) = response else {
        return Err(Error::InvalidEntityStoreResponse);
    };

    if deleted.is_empty() {
        return Ok(StatusCode::NO_CONTENT.into_response());
    }

    Ok((StatusCode::OK, Json(deleted)).into_response())
}

async fn list_entities(
    State(state): State<AgentState>,
    Query(params): Query<ListParams>,
) -> Result<Json<Vec<EntityMetadata>>, Error> {
    let filters = params.try_into()?;
    let response = state
        .entity_store_handle
        .clone()
        .await_response(EntityStoreRequest::List(filters))
        .await?;

    let EntityStoreResponse::List(entities) = response else {
        return Err(Error::InvalidEntityStoreResponse);
    };

    Ok(Json(entities))
}

#[cfg(test)]
mod tests {
    use super::AgentState;
    use crate::entity_manager::server::EntityStoreRequest;
    use crate::entity_manager::server::EntityStoreResponse;
    use crate::http_server::entity_store::entity_store_router;
    use assert_json_diff::assert_json_eq;
    use axum::body::Body;
    use axum::Router;
    use http_body_util::BodyExt as _;
    use hyper::Method;
    use hyper::Request;
    use hyper::StatusCode;
    use serde_json::json;
    use serde_json::Map;
    use serde_json::Value;
    use std::collections::HashSet;
    use tedge_actors::Builder;
    use tedge_actors::ClientMessageBox;
    use tedge_actors::MessageReceiver;
    use tedge_actors::ServerMessageBox;
    use tedge_actors::ServerMessageBoxBuilder;
    use tedge_api::entity::EntityMetadata;
    use tedge_api::entity::EntityType;
    use tedge_api::entity_store;
    use tedge_api::entity_store::EntityRegistrationMessage;
    use tedge_api::mqtt_topics::EntityTopicId;
    use tedge_test_utils::fs::TempTedgeDir;
    use tower::Service;

    #[tokio::test]
    async fn entity_get() {
        let TestHandle {
            mut app,
            mut entity_store_box,
        } = setup();

        // Mock entity store actor response
        tokio::spawn(async move {
            if let Some(mut req) = entity_store_box.recv().await {
                if let EntityStoreRequest::Get(topic_id) = req.request {
                    if topic_id == EntityTopicId::default_child_device("test-child").unwrap() {
                        let entity =
                            EntityMetadata::child_device("test-child".to_string()).unwrap();
                        req.reply_to
                            .send(EntityStoreResponse::Get(Some(entity)))
                            .await
                            .unwrap();
                    }
                }
            }
        });

        let topic_id = "device/test-child//";
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/entities/{topic_id}"))
            .body(Body::empty())
            .expect("request builder");

        let response = app.call(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let entity: EntityMetadata = serde_json::from_slice(&body).unwrap();
        assert_eq!(entity.topic_id.as_str(), topic_id);
        assert_eq!(entity.r#type, EntityType::ChildDevice);
    }

    #[tokio::test]
    async fn get_unknown_entity() {
        let TestHandle {
            mut app,
            mut entity_store_box,
        } = setup();

        // Mock entity store actor response
        tokio::spawn(async move {
            if let Some(mut req) = entity_store_box.recv().await {
                if let EntityStoreRequest::Get(topic_id) = req.request {
                    if topic_id == EntityTopicId::default_child_device("test-child").unwrap() {
                        req.reply_to
                            .send(EntityStoreResponse::Get(None))
                            .await
                            .unwrap();
                    }
                }
            }
        });

        let topic_id = "device/test-child//";
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/entities/{topic_id}"))
            .body(Body::empty())
            .expect("request builder");

        let response = app.call(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let entity: Value = serde_json::from_slice(&body).unwrap();
        assert_json_eq!(
            entity,
            json!( {"error":"Entity not found with topic id: device/test-child//"})
        );
    }

    #[tokio::test]
    async fn entity_post() {
        let TestHandle {
            mut app,
            mut entity_store_box,
        } = setup();

        // Mock entity store actor response
        tokio::spawn(async move {
            if let Some(mut req) = entity_store_box.recv().await {
                if let EntityStoreRequest::Create(entity) = req.request {
                    if entity.topic_id == EntityTopicId::default_child_device("test-child").unwrap()
                        && entity.r#type == EntityType::ChildDevice
                    {
                        req.reply_to
                            .send(EntityStoreResponse::Create(Ok(vec![])))
                            .await
                            .unwrap();
                    }
                }
            }
        });

        let entity = EntityRegistrationMessage {
            topic_id: EntityTopicId::default_child_device("test-child").unwrap(),
            external_id: Some("test-child".into()),
            r#type: EntityType::ChildDevice,
            parent: None,
            other: Map::new(),
        };
        let payload = serde_json::to_string(&entity).unwrap();

        let req = Request::builder()
            .method(Method::POST)
            .uri("/v1/entities")
            .header("Content-Type", "application/json")
            .body(Body::from(payload))
            .expect("request builder");

        let response = app.call(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let entity: Value = serde_json::from_slice(&body).unwrap();
        assert_json_eq!(entity, json!( {"@topic-id": "device/test-child//"}));
    }

    #[tokio::test]
    async fn entity_post_duplicate() {
        let TestHandle {
            mut app,
            mut entity_store_box,
        } = setup();

        // Mock entity store actor response
        tokio::spawn(async move {
            let topic_id = EntityTopicId::default_child_device("test-child").unwrap();
            if let Some(mut req) = entity_store_box.recv().await {
                if let EntityStoreRequest::Create(entity) = req.request {
                    if entity.topic_id == topic_id && entity.r#type == EntityType::ChildDevice {
                        req.reply_to
                            .send(EntityStoreResponse::Create(Err(
                                entity_store::Error::EntityAlreadyRegistered(topic_id),
                            )))
                            .await
                            .unwrap();
                    }
                }
            }
        });

        let entity = EntityRegistrationMessage {
            topic_id: EntityTopicId::default_child_device("test-child").unwrap(),
            external_id: Some("test-child".into()),
            r#type: EntityType::ChildDevice,
            parent: None,
            other: Map::new(),
        };
        let payload = serde_json::to_string(&entity).unwrap();

        let req = Request::builder()
            .method(Method::POST)
            .uri("/v1/entities")
            .header("Content-Type", "application/json")
            .body(Body::from(payload))
            .expect("request builder");

        let response = app.call(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let entity: Value = serde_json::from_slice(&body).unwrap();
        assert_json_eq!(
            entity,
            json!( {"error":"An entity with topic id: device/test-child// is already registered"})
        );
    }

    #[tokio::test]
    async fn entity_post_bad_parent() {
        let TestHandle {
            mut app,
            mut entity_store_box,
        } = setup();

        // Mock entity store actor response
        tokio::spawn(async move {
            let topic_id = EntityTopicId::default_child_device("test-child").unwrap();
            if let Some(mut req) = entity_store_box.recv().await {
                if let EntityStoreRequest::Create(entity) = req.request {
                    if entity.topic_id == topic_id && entity.r#type == EntityType::ChildDevice {
                        req.reply_to
                            .send(EntityStoreResponse::Create(Err(
                                entity_store::Error::NoParent(
                                    "test-child".to_string().into_boxed_str(),
                                ),
                            )))
                            .await
                            .unwrap();
                    }
                }
            }
        });

        let entity = EntityRegistrationMessage {
            topic_id: EntityTopicId::default_child_device("test-child").unwrap(),
            external_id: Some("test-child".into()),
            r#type: EntityType::ChildDevice,
            parent: None,
            other: Map::new(),
        };
        let payload = serde_json::to_string(&entity).unwrap();

        let req = Request::builder()
            .method(Method::POST)
            .uri("/v1/entities")
            .header("Content-Type", "application/json")
            .body(Body::from(payload))
            .expect("request builder");

        let response = app.call(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let entity: Value = serde_json::from_slice(&body).unwrap();
        assert_json_eq!(
            entity,
            json!( {"error":"Specified parent \"test-child\" does not exist in the store"})
        );
    }

    #[tokio::test]
    async fn entity_delete() {
        let TestHandle {
            mut app,
            mut entity_store_box,
        } = setup();

        // Mock entity store actor response
        tokio::spawn(async move {
            if let Some(mut req) = entity_store_box.recv().await {
                if let EntityStoreRequest::Delete(topic_id) = req.request {
                    let target_entity =
                        EntityMetadata::child_device("test-child".to_string()).unwrap();
                    if topic_id == target_entity.topic_id {
                        req.reply_to
                            .send(EntityStoreResponse::Delete(vec![target_entity]))
                            .await
                            .unwrap();
                    }
                }
            }
        });

        let topic_id = "device/test-child//";
        let req = Request::builder()
            .method(Method::DELETE)
            .uri(format!("/v1/entities/{topic_id}"))
            .body(Body::empty())
            .expect("request builder");

        let response = app.call(req).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let deleted: Vec<EntityMetadata> = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            deleted,
            vec![EntityMetadata::child_device("test-child".to_string()).unwrap()]
        );
    }

    #[tokio::test]
    async fn delete_unknown_entity_returns_no_content() {
        let TestHandle {
            mut app,
            mut entity_store_box,
        } = setup();

        // Mock entity store actor response
        tokio::spawn(async move {
            if let Some(mut req) = entity_store_box.recv().await {
                if let EntityStoreRequest::Delete(topic_id) = req.request {
                    if topic_id == EntityTopicId::default_child_device("test-child").unwrap() {
                        req.reply_to
                            .send(EntityStoreResponse::Delete(vec![]))
                            .await
                            .unwrap();
                    }
                }
            }
        });

        let topic_id = "device/test-child//";
        let req = Request::builder()
            .method(Method::DELETE)
            .uri(format!("/v1/entities/{topic_id}"))
            .body(Body::empty())
            .expect("request builder");

        let response = app.call(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn entity_list() {
        let TestHandle {
            mut app,
            mut entity_store_box,
        } = setup();

        // Mock entity store actor response
        tokio::spawn(async move {
            if let Some(mut req) = entity_store_box.recv().await {
                if let EntityStoreRequest::List(_) = req.request {
                    req.reply_to
                        .send(EntityStoreResponse::List(vec![
                            EntityMetadata::main_device(),
                            EntityMetadata::child_device("child0".to_string()).unwrap(),
                            EntityMetadata::child_device("child1".to_string()).unwrap(),
                        ]))
                        .await
                        .unwrap();
                }
            }
        });

        let req = Request::builder()
            .method(Method::GET)
            .uri("/v1/entities")
            .body(Body::empty())
            .expect("request builder");

        let response = app.call(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let entities: Vec<EntityMetadata> = serde_json::from_slice(&body).unwrap();

        let entity_set = entities
            .iter()
            .map(|e| e.topic_id.as_str())
            .collect::<HashSet<_>>();
        assert!(entity_set.contains("device/main//"));
        assert!(entity_set.contains("device/child0//"));
        assert!(entity_set.contains("device/child1//"));
    }

    #[tokio::test]
    async fn entity_list_unknown_entity() {
        let TestHandle {
            mut app,
            mut entity_store_box,
        } = setup();

        // Mock entity store actor response
        tokio::spawn(async move {
            if let Some(mut req) = entity_store_box.recv().await {
                if let EntityStoreRequest::List(_) = req.request {
                    req.reply_to
                        .send(EntityStoreResponse::List(vec![]))
                        .await
                        .unwrap();
                }
            }
        });

        let req = Request::builder()
            .method(Method::GET)
            .uri("/v1/entities")
            .body(Body::empty())
            .expect("request builder");

        let response = app.call(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let entities: Vec<EntityMetadata> = serde_json::from_slice(&body).unwrap();
        assert!(entities.is_empty());
    }

    #[tokio::test]
    async fn entity_list_query_parameters() {
        let TestHandle {
            mut app,
            mut entity_store_box,
        } = setup();

        // Mock entity store actor response
        tokio::spawn(async move {
            if let Some(mut req) = entity_store_box.recv().await {
                if let EntityStoreRequest::List(_) = req.request {
                    req.reply_to
                        .send(EntityStoreResponse::List(vec![
                            EntityMetadata::child_device("child00".to_string()).unwrap(),
                            EntityMetadata::child_device("child01".to_string()).unwrap(),
                        ]))
                        .await
                        .unwrap();
                }
            }
        });

        let req = Request::builder()
            .method(Method::GET)
            .uri("/v1/entities?parent=device/child0//&type=child-device")
            .body(Body::empty())
            .expect("request builder");

        let response = app.call(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let entities: Vec<EntityMetadata> = serde_json::from_slice(&body).unwrap();

        let entity_set = entities
            .iter()
            .map(|e| e.topic_id.as_str())
            .collect::<HashSet<_>>();
        assert!(entity_set.contains("device/child00//"));
        assert!(entity_set.contains("device/child01//"));
    }

    #[tokio::test]
    async fn entity_list_empty_query_param() {
        let TestHandle {
            mut app,
            mut entity_store_box,
        } = setup();
        // Mock entity store actor response
        tokio::spawn(async move {
            while let Some(mut req) = entity_store_box.recv().await {
                if let EntityStoreRequest::List(_) = req.request {
                    req.reply_to
                        .send(EntityStoreResponse::List(vec![]))
                        .await
                        .unwrap();
                }
            }
        });

        for param in ["root=", "parent=", "type="].into_iter() {
            let uri = format!("/v1/entities?{}", param);
            let req = Request::builder()
                .method(Method::GET)
                .uri(uri)
                .body(Body::empty())
                .expect("request builder");

            let response = app.call(req).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }

        let req = Request::builder()
            .method(Method::GET)
            .uri("/v1/entities?root=&parent=&type=")
            .body(Body::empty())
            .expect("request builder");

        let response = app.call(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn entity_list_bad_query_param() {
        let TestHandle {
            mut app,
            entity_store_box: _, // Not used
        } = setup();

        let req = Request::builder()
            .method(Method::GET)
            .uri("/v1/entities?parent=an/invalid/topic/id/")
            .body(Body::empty())
            .expect("request builder");

        let response = app.call(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let entity: Value = serde_json::from_slice(&body).unwrap();
        assert_json_eq!(
            entity,
            json!( {"error":"An entity topic identifier has at most 4 segments"})
        );
    }

    #[tokio::test]
    async fn entity_list_bad_query_parameter_combination() {
        let TestHandle {
            mut app,
            entity_store_box: _, // Not used
        } = setup();

        let req = Request::builder()
            .method(Method::GET)
            .uri("/v1/entities?root=device/some/topic/id&parent=device/another/topic/id")
            .body(Body::empty())
            .expect("request builder");

        let response = app.call(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let entity: Value = serde_json::from_slice(&body).unwrap();
        assert_json_eq!(
            entity,
            json!( {"error":"The provided parameters: root and parent are mutually exclusive. Use either one."})
        );
    }

    struct TestHandle {
        app: Router,
        entity_store_box: ServerMessageBox<EntityStoreRequest, EntityStoreResponse>,
    }

    fn setup() -> TestHandle {
        let ttd: TempTedgeDir = TempTedgeDir::new();
        let file_transfer_dir = ttd.utf8_path_buf();

        let mut entity_store_box = ServerMessageBoxBuilder::new("EntityStoreBox", 16);
        let entity_store_handle = ClientMessageBox::new(&mut entity_store_box);

        let agent_state = AgentState {
            file_transfer_dir,
            entity_store_handle,
        };
        // TODO: Add a timeout to this router. Attempts to add a tower_http::timer::TimeoutLayer as a layer failed.
        let app: Router = entity_store_router(agent_state);

        TestHandle {
            app,
            entity_store_box: entity_store_box.build(),
        }
    }
}
