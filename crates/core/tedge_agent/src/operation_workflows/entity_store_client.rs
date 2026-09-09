//! A client of the entity store
//!
//! The store runs on the main device only:
//! the agent running there owns it, and the agent of a child device reaches it over the network.

use crate::entity_manager::server::EntityStoreRequest;
use crate::entity_manager::server::EntityStoreResponse;
use hyper::StatusCode;
use std::time::Duration;
use tedge_actors::ChannelError;
use tedge_actors::ClientMessageBox;
use tedge_actors::Service;
use tedge_api::entity::EntityMetadata;
use tedge_api::file_transfer_url::EntityStoreUrls;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_http_ext::HttpError;
use tedge_http_ext::HttpRequest;
use tedge_http_ext::HttpRequestBuilder;
use tedge_http_ext::HttpResponseExt;
use tedge_http_ext::HttpResult;
use tracing::warn;

// Without it, a request left unanswered would never be retried, as the HTTP client has no timeout
const LOOKUP_TIMEOUT: Duration = Duration::from_secs(10);

const RETRY_DELAYS: [Duration; 9] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(4),
    Duration::from_secs(8),
    Duration::from_secs(16),
    Duration::from_secs(32),
    Duration::from_secs(64),
    Duration::from_secs(128),
    Duration::from_secs(256),
];

#[derive(Debug, thiserror::Error)]
pub enum EntityStoreClientError {
    #[error(transparent)]
    ChannelError(#[from] ChannelError),

    #[error(transparent)]
    HttpError(#[from] HttpError),

    #[error("Unexpected status {status} returned by the entity store for {url}")]
    UnexpectedStatus { url: String, status: StatusCode },

    #[error("Unexpected response returned by the entity store for {topic_id}")]
    UnexpectedResponse { topic_id: EntityTopicId },

    #[error("No response from the entity store for {topic_id} within {timeout:?}")]
    Timeout {
        topic_id: EntityTopicId,
        timeout: Duration,
    },
}

#[derive(Clone)]
pub enum EntityStoreClient {
    /// The store of the agent running this client
    Local(ClientMessageBox<EntityStoreRequest, EntityStoreResponse>),

    /// The store of the main device, reached over its REST API
    Remote {
        urls: EntityStoreUrls,
        http: ClientMessageBox<HttpRequest, HttpResult>,
    },
}

impl EntityStoreClient {
    pub fn local(store: &mut impl Service<EntityStoreRequest, EntityStoreResponse>) -> Self {
        EntityStoreClient::Local(ClientMessageBox::new(store))
    }

    pub fn remote(urls: EntityStoreUrls, http: &mut impl Service<HttpRequest, HttpResult>) -> Self {
        EntityStoreClient::Remote {
            urls,
            http: ClientMessageBox::new(http),
        }
    }

    pub async fn get_with_retries(
        &mut self,
        topic_id: &EntityTopicId,
    ) -> Result<Option<EntityMetadata>, EntityStoreClientError> {
        let mut delays = RETRY_DELAYS.iter();
        loop {
            match self.get(topic_id).await {
                Ok(entity) => return Ok(entity),
                Err(err) => match delays.next() {
                    Some(delay) => {
                        warn!("Failed to read the registration data of {topic_id}, retrying in {delay:?}: {err}");
                        tokio::time::sleep(*delay).await;
                    }
                    None => return Err(err),
                },
            }
        }
    }

    pub async fn get(
        &mut self,
        topic_id: &EntityTopicId,
    ) -> Result<Option<EntityMetadata>, EntityStoreClientError> {
        match tokio::time::timeout(LOOKUP_TIMEOUT, self.fetch(topic_id)).await {
            Ok(entity) => entity,
            Err(_elapsed) => Err(EntityStoreClientError::Timeout {
                topic_id: topic_id.clone(),
                timeout: LOOKUP_TIMEOUT,
            }),
        }
    }

    async fn fetch(
        &mut self,
        topic_id: &EntityTopicId,
    ) -> Result<Option<EntityMetadata>, EntityStoreClientError> {
        match self {
            EntityStoreClient::Local(store) => {
                let request = EntityStoreRequest::Get(topic_id.clone());
                match store.await_response(request).await? {
                    EntityStoreResponse::Get(entity) => Ok(entity),
                    _ => Err(EntityStoreClientError::UnexpectedResponse {
                        topic_id: topic_id.clone(),
                    }),
                }
            }

            EntityStoreClient::Remote { urls, http } => {
                let url = urls.for_entity(topic_id);
                let request = HttpRequestBuilder::get(&url).build()?;
                let response = http.await_response(request).await??;

                match response.status() {
                    StatusCode::NOT_FOUND => Ok(None),
                    status if status.is_success() => Ok(Some(response.json().await?)),
                    status => Err(EntityStoreClientError::UnexpectedStatus { url, status }),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tedge_actors::test_helpers::FakeServerBox;
    use tedge_actors::Builder;
    use tedge_actors::MessageReceiver;
    use tedge_actors::Sender;
    use tedge_api::entity::EntityType;
    use tedge_api::file_transfer_url::Protocol;
    use tedge_http_ext::test_helpers::HttpResponseBuilder;

    #[tokio::test]
    async fn a_remote_store_returns_the_registration_data() {
        let (mut client, mut http) = spawn_remote_client();
        let topic_id: EntityTopicId = "device/main/service/collectd".parse().unwrap();
        let expected = EntityMetadata::new(topic_id.clone(), EntityType::Service)
            .with_parent(EntityTopicId::default_main_device());

        let lookup = tokio::spawn(async move { client.get(&topic_id).await });

        http.recv().await.unwrap();
        http.send(
            HttpResponseBuilder::new()
                .status(200)
                .json(&expected)
                .build(),
        )
        .await
        .unwrap();

        assert_eq!(lookup.await.unwrap().unwrap(), Some(expected));
    }

    #[tokio::test]
    async fn a_remote_store_returns_none_for_an_unknown_entity() {
        let (mut client, mut http) = spawn_remote_client();
        let topic_id: EntityTopicId = "device/main/service/unknown".parse().unwrap();

        let lookup = tokio::spawn(async move { client.get(&topic_id).await });

        http.recv().await.unwrap();
        http.send(HttpResponseBuilder::new().status(404).build())
            .await
            .unwrap();

        assert_eq!(lookup.await.unwrap().unwrap(), None);
    }

    #[tokio::test]
    async fn any_other_status_of_a_remote_store_is_an_error() {
        let (mut client, mut http) = spawn_remote_client();
        let topic_id: EntityTopicId = "device/main/service/collectd".parse().unwrap();

        let lookup = tokio::spawn(async move { client.get(&topic_id).await });

        http.recv().await.unwrap();
        http.send(HttpResponseBuilder::new().status(500).build())
            .await
            .unwrap();

        assert!(lookup.await.unwrap().is_err());
    }

    #[tokio::test(start_paused = true)]
    async fn a_failed_lookup_is_retried_until_it_succeeds() {
        let (mut client, mut http) = spawn_remote_client();
        let topic_id: EntityTopicId = "device/main/service/collectd".parse().unwrap();
        let expected = EntityMetadata::new(topic_id.clone(), EntityType::Service)
            .with_parent(EntityTopicId::default_main_device());

        let lookup = tokio::spawn(async move { client.get_with_retries(&topic_id).await });

        http.recv().await.unwrap();
        http.send(HttpResponseBuilder::new().status(500).build())
            .await
            .unwrap();
        http.recv().await.unwrap();
        http.send(
            HttpResponseBuilder::new()
                .status(200)
                .json(&expected)
                .build(),
        )
        .await
        .unwrap();

        assert_eq!(lookup.await.unwrap().unwrap(), Some(expected));
    }

    #[tokio::test(start_paused = true)]
    async fn a_lookup_gives_up_after_the_last_retry() {
        let (mut client, mut http) = spawn_remote_client();
        let topic_id: EntityTopicId = "device/main/service/collectd".parse().unwrap();

        let lookup = tokio::spawn(async move { client.get_with_retries(&topic_id).await });

        for _ in 0..=RETRY_DELAYS.len() {
            http.recv().await.unwrap();
            http.send(HttpResponseBuilder::new().status(500).build())
                .await
                .unwrap();
        }

        assert!(lookup.await.unwrap().is_err());
    }

    #[tokio::test]
    async fn a_local_store_returns_the_registration_data() {
        let (mut client, mut store) = spawn_local_client();
        let topic_id: EntityTopicId = "device/main/service/collectd".parse().unwrap();
        let expected = EntityMetadata::new(topic_id.clone(), EntityType::Service)
            .with_parent(EntityTopicId::default_main_device());

        let lookup = tokio::spawn(async move { client.get(&topic_id).await });

        store.recv().await.unwrap();
        store
            .send(EntityStoreResponse::Get(Some(expected.clone())))
            .await
            .unwrap();

        assert_eq!(lookup.await.unwrap().unwrap(), Some(expected));
    }

    #[tokio::test]
    async fn a_local_store_returns_none_for_an_unknown_entity() {
        let (mut client, mut store) = spawn_local_client();
        let topic_id: EntityTopicId = "device/main/service/unknown".parse().unwrap();

        let lookup = tokio::spawn(async move { client.get(&topic_id).await });

        store.recv().await.unwrap();
        store.send(EntityStoreResponse::Get(None)).await.unwrap();

        assert_eq!(lookup.await.unwrap().unwrap(), None);
    }

    #[tokio::test]
    async fn a_local_store_returns_the_twin_data() {
        let (mut client, mut store) = spawn_local_client();
        let topic_id: EntityTopicId = "device/main/service/nodered".parse().unwrap();
        let mut expected = EntityMetadata::new(topic_id.clone(), EntityType::Service)
            .with_parent(EntityTopicId::default_main_device());
        expected
            .twin_data
            .insert("type".to_string(), json!("systemd"));

        let lookup = tokio::spawn(async move { client.get(&topic_id).await });

        store.recv().await.unwrap();
        store
            .send(EntityStoreResponse::Get(Some(expected)))
            .await
            .unwrap();

        let entity = lookup.await.unwrap().unwrap().unwrap();
        assert_eq!(entity.twin_data.get("type").unwrap(), "systemd");
    }

    #[tokio::test(start_paused = true)]
    async fn a_store_that_never_answers_times_out() {
        let (mut client, mut http) = spawn_remote_client();
        let topic_id: EntityTopicId = "device/main/service/collectd".parse().unwrap();

        let lookup = tokio::spawn(async move { client.get(&topic_id).await });

        // The request is received, but left unanswered
        http.recv().await.unwrap();

        assert!(matches!(
            lookup.await.unwrap(),
            Err(EntityStoreClientError::Timeout { .. })
        ));
    }

    fn spawn_remote_client() -> (EntityStoreClient, FakeServerBox<HttpRequest, HttpResult>) {
        let mut http = FakeServerBox::builder();
        let client = EntityStoreClient::remote(
            EntityStoreUrls::new("127.0.0.1:8000".into(), Protocol::Http),
            &mut http,
        );
        (client, http.build())
    }

    fn spawn_local_client() -> (
        EntityStoreClient,
        FakeServerBox<EntityStoreRequest, EntityStoreResponse>,
    ) {
        let mut store = FakeServerBox::builder();
        let client = EntityStoreClient::local(&mut store);
        (client, store.build())
    }
}
