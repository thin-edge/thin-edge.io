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
use super::http_rest::AgentState;
use crate::entity_manager::server::EntityStoreRequest;
use crate::entity_manager::server::EntityStoreResponse;
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
use tedge_actors::Sender;
use tedge_api::entity_store;
use tedge_api::entity_store::EntityMetadata;
use tedge_api::entity_store::EntityRegistrationMessage;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::TopicIdError;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::QoS;

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
            Error::EntityStoreError(_) => StatusCode::BAD_REQUEST,
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
        .route(
            "/v1/entities/*path",
            post(register_entity)
                .get(get_entity)
                .delete(deregister_entity),
        )
        .with_state(state)
}

async fn register_entity(
    State(state): State<AgentState>,
    Json(entity): Json<EntityRegistrationMessage>,
) -> Result<StatusCode, Error> {
    let response = state
        .entity_store_handle
        .clone()
        .await_response(EntityStoreRequest::Create(entity.clone()))
        .await?;
    let EntityStoreResponse::Create(result) = response else {
        return Err(Error::InvalidEntityStoreResponse);
    };

    if !result?.0.is_empty() {
        let message = entity.to_mqtt_message(&state.mqtt_schema);
        state.mqtt_publisher.clone().send(message).await?;
    }
    Ok(StatusCode::OK)
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
) -> Result<StatusCode, Error> {
    let topic_id = EntityTopicId::from_str(&path)?;

    let response = state
        .entity_store_handle
        .clone()
        .await_response(EntityStoreRequest::Delete(topic_id.clone()))
        .await?;

    let EntityStoreResponse::Delete(deleted) = response else {
        return Err(Error::InvalidEntityStoreResponse);
    };

    for topic_id in deleted {
        let topic = state
            .mqtt_schema
            .topic_for(&topic_id, &Channel::EntityMetadata);
        let clear_entity_msg = MqttMessage::new(&topic, "")
            .with_retain()
            .with_qos(QoS::AtLeastOnce);

        state.mqtt_publisher.clone().send(clear_entity_msg).await?;
    }
    Ok(StatusCode::OK)
}

#[cfg(test)]
mod tests {
    use super::AgentState;
    use crate::entity_manager::server::EntityStoreRequest;
    use crate::entity_manager::server::EntityStoreResponse;
    use crate::file_transfer_server::entity_store::entity_store_router;
    use axum::Router;
    use hyper::Body;
    use hyper::Method;
    use hyper::Request;
    use hyper::StatusCode;
    use serde_json::Map;
    use std::time::Duration;
    use tedge_actors::test_helpers::MessageReceiverExt;
    use tedge_actors::test_helpers::TimedMessageBox;
    use tedge_actors::Builder;
    use tedge_actors::ClientMessageBox;
    use tedge_actors::LoggingSender;
    use tedge_actors::MessageReceiver;
    use tedge_actors::MessageSink;
    use tedge_actors::ServerMessageBox;
    use tedge_actors::ServerMessageBoxBuilder;
    use tedge_actors::SimpleMessageBox;
    use tedge_actors::SimpleMessageBoxBuilder;
    use tedge_api::entity_store::EntityMetadata;
    use tedge_api::entity_store::EntityRegistrationMessage;
    use tedge_api::entity_store::EntityType;
    use tedge_api::mqtt_topics::EntityTopicId;
    use tedge_api::mqtt_topics::MqttSchema;
    use tedge_mqtt_ext::test_helpers::assert_received_contains_str;
    use tedge_mqtt_ext::MqttMessage;
    use tedge_test_utils::fs::TempTedgeDir;
    use tower::Service;

    const TEST_TIMEOUT_MS: Duration = Duration::from_millis(3000);

    #[tokio::test]
    async fn entity_get() {
        let TestHandle {
            mut app,
            mqtt_box: _,
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
            mqtt_box: _,
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
    async fn entity_put() {
        let TestHandle {
            mut app,
            mut mqtt_box,
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
                            .send(EntityStoreResponse::Create(Ok((
                                vec![EntityTopicId::default_main_device()],
                                vec![],
                            ))))
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

        let topic_id = "device/test-child//";
        let req = Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/entities/{topic_id}"))
            .header("Content-Type", "application/json")
            .body(Body::from(payload))
            .expect("request builder");

        let response = app.call(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let message = mqtt_box.recv().await.unwrap();
        let received = EntityRegistrationMessage::new(&message).unwrap();
        assert_eq!(received, entity);
    }

    #[tokio::test]
    async fn entity_delete() {
        let TestHandle {
            mut app,
            mut mqtt_box,
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

        assert_received_contains_str(&mut mqtt_box, [("te/device/test-child//", "")]).await;
    }

    #[tokio::test]
    async fn delete_unknown_entity_is_ok() {
        let TestHandle {
            mut app,
            mqtt_box: _,
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

    struct TestHandle {
        app: Router,
        mqtt_box: TimedMessageBox<SimpleMessageBox<MqttMessage, MqttMessage>>,
        entity_store_box: ServerMessageBox<EntityStoreRequest, EntityStoreResponse>,
    }

    fn setup() -> TestHandle {
        let ttd: TempTedgeDir = TempTedgeDir::new();
        let file_transfer_dir = ttd.utf8_path_buf();

        let mqtt_box = SimpleMessageBoxBuilder::new("MQTT", 10);

        let mut entity_store_box = ServerMessageBoxBuilder::new("EntityStoreBox", 16);
        let entity_store_handle = ClientMessageBox::new(&mut entity_store_box);

        let agent_state = AgentState {
            file_transfer_dir,
            entity_store_handle,
            mqtt_schema: MqttSchema::default(),
            mqtt_publisher: LoggingSender::new("MQTT".to_string(), mqtt_box.get_sender()),
        };
        // TODO: Add a timeout to this router. Attempts to add a tower_http::timer::TimeoutLayer as a layer failed.
        let app: Router = entity_store_router(agent_state);

        TestHandle {
            app,
            mqtt_box: mqtt_box.build().with_timeout(TEST_TIMEOUT_MS),
            entity_store_box: entity_store_box.build(),
        }
    }
}
