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
use axum::extract::State;
use axum::response::IntoResponse;
use axum::response::Response;
use axum::routing::get;
use axum::routing::post;
use axum::Json;
use axum::Router;
use hyper::StatusCode;
use serde_json::json;
use std::str::FromStr;
use tedge_api::entity::EntityMetadata;
use tedge_api::entity_store;
use tedge_api::entity_store::EntityRegistrationMessage;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::TopicIdError;

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
        };
        let error_message = self.to_string();

        (status_code, error_message).into_response()
    }
}

pub(crate) fn entity_store_router(state: AgentState) -> Router {
    Router::new()
        .route("/v1/entities", post(register_entity).get(list_entities))
        .route(
            "/v1/entities/*path",
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
) -> Result<Json<Vec<EntityTopicId>>, Error> {
    let topic_id = EntityTopicId::from_str(&path)?;

    let response = state
        .entity_store_handle
        .clone()
        .await_response(EntityStoreRequest::Delete(topic_id.clone()))
        .await?;

    let EntityStoreResponse::Delete(deleted) = response else {
        return Err(Error::InvalidEntityStoreResponse);
    };

    Ok(Json(deleted))
}

async fn list_entities(
    State(state): State<AgentState>,
) -> Result<Json<Vec<EntityMetadata>>, Error> {
    let response = state
        .entity_store_handle
        .clone()
        .await_response(EntityStoreRequest::List(None))
        .await?;

    let EntityStoreResponse::List(entities) = response else {
        return Err(Error::InvalidEntityStoreResponse);
    };

    Ok(Json(entities?))
}

#[cfg(test)]
mod tests {
    use super::AgentState;
    use crate::entity_manager::server::EntityStoreRequest;
    use crate::entity_manager::server::EntityStoreResponse;
    use crate::http_server::entity_store::entity_store_router;
    use assert_json_diff::assert_json_eq;
    use axum::Router;
    use hyper::Body;
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

        let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
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

        let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
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
                    let target_topic_id =
                        EntityTopicId::default_child_device("test-child").unwrap();
                    if topic_id == target_topic_id {
                        req.reply_to
                            .send(EntityStoreResponse::Delete(vec![target_topic_id]))
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
        let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
        let deleted: Vec<EntityTopicId> = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            deleted,
            vec![EntityTopicId::default_child_device("test-child").unwrap()]
        );
    }

    #[tokio::test]
    async fn delete_unknown_entity_is_ok() {
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
        assert_eq!(response.status(), StatusCode::OK);
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
                        .send(EntityStoreResponse::List(Ok(vec![
                            EntityMetadata::main_device("main".to_string()),
                            EntityMetadata::child_device("child0".to_string()).unwrap(),
                            EntityMetadata::child_device("child1".to_string()).unwrap(),
                        ])))
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

        let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
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
                        .send(EntityStoreResponse::List(Err(
                            entity_store::Error::UnknownEntity("unknown".to_string()),
                        )))
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
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
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
