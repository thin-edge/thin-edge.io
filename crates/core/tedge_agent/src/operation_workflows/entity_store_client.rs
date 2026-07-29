//! A client of the entity store REST API
//!
//! The store runs on the main device only, so a child device's agent reaches it over the network.

use hyper::StatusCode;
use std::sync::Arc;
use std::time::Duration;
use tedge_actors::ChannelError;
use tedge_actors::ClientMessageBox;
use tedge_api::entity::EntityMetadata;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_http_ext::HttpError;
use tedge_http_ext::HttpRequest;
use tedge_http_ext::HttpRequestBuilder;
use tedge_http_ext::HttpResponseExt;
use tedge_http_ext::HttpResult;

/// Without it, a store that never answers would stall the actor's main loop
const LOOKUP_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, thiserror::Error)]
pub enum EntityStoreClientError {
    #[error(transparent)]
    ChannelError(#[from] ChannelError),

    #[error(transparent)]
    HttpError(#[from] HttpError),

    #[error("Unexpected status {status} returned by the entity store for {url}")]
    UnexpectedStatus { url: String, status: StatusCode },

    #[error("No response from the entity store for {url} within {timeout:?}")]
    Timeout { url: String, timeout: Duration },
}

pub struct EntityStoreClient {
    entities_url: Arc<str>,
    http: ClientMessageBox<HttpRequest, HttpResult>,
}

impl EntityStoreClient {
    pub fn new(entities_url: Arc<str>, http: ClientMessageBox<HttpRequest, HttpResult>) -> Self {
        Self { entities_url, http }
    }

    /// The registration data of an entity, `None` if no such entity is registered
    ///
    /// Fails with [EntityStoreClientError::Timeout] if the store has not answered within
    /// [LOOKUP_TIMEOUT], so that an unresponsive store cannot block the caller for ever.
    pub async fn get(
        &mut self,
        topic_id: &EntityTopicId,
    ) -> Result<Option<EntityMetadata>, EntityStoreClientError> {
        let url = self.entity_url(topic_id);
        match tokio::time::timeout(LOOKUP_TIMEOUT, self.fetch(&url)).await {
            Ok(entity) => entity,
            Err(_elapsed) => Err(EntityStoreClientError::Timeout {
                url,
                timeout: LOOKUP_TIMEOUT,
            }),
        }
    }

    async fn fetch(&mut self, url: &str) -> Result<Option<EntityMetadata>, EntityStoreClientError> {
        let request = HttpRequestBuilder::get(url).build()?;
        let response = self.http.await_response(request).await??;

        match response.status() {
            StatusCode::NOT_FOUND => Ok(None),
            status if status.is_success() => Ok(Some(response.json().await?)),
            status => Err(EntityStoreClientError::UnexpectedStatus {
                url: url.to_string(),
                status,
            }),
        }
    }

    fn entity_url(&self, topic_id: &EntityTopicId) -> String {
        format!("{}/{}", self.entities_url, topic_id.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tedge_actors::test_helpers::FakeServerBox;
    use tedge_actors::Builder;
    use tedge_actors::MessageReceiver;
    use tedge_actors::Sender;
    use tedge_api::entity::EntityType;
    use tedge_http_ext::test_helpers::HttpResponseBuilder;

    #[tokio::test]
    async fn a_service_is_fetched_with_its_full_topic_id() {
        let (mut client, mut http) = spawn_client();
        let topic_id: EntityTopicId = "device/main/service/collectd".parse().unwrap();

        tokio::spawn(async move { client.get(&topic_id).await });

        let request = http.recv().await.unwrap();
        assert_eq!(
            request.uri().to_string(),
            "http://127.0.0.1:8000/te/v1/entities/device/main/service/collectd"
        );
    }

    #[tokio::test]
    async fn a_device_is_fetched_with_its_empty_segments() {
        let (mut client, mut http) = spawn_client();
        let topic_id = EntityTopicId::default_child_device("child01").unwrap();

        tokio::spawn(async move { client.get(&topic_id).await });

        let request = http.recv().await.unwrap();
        assert_eq!(
            request.uri().to_string(),
            "http://127.0.0.1:8000/te/v1/entities/device/child01//"
        );
    }

    #[tokio::test]
    async fn the_registration_data_is_returned_on_success() {
        let (mut client, mut http) = spawn_client();
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
    async fn an_unknown_entity_is_returned_as_none() {
        let (mut client, mut http) = spawn_client();
        let topic_id: EntityTopicId = "device/main/service/unknown".parse().unwrap();

        let lookup = tokio::spawn(async move { client.get(&topic_id).await });

        http.recv().await.unwrap();
        http.send(HttpResponseBuilder::new().status(404).build())
            .await
            .unwrap();

        assert_eq!(lookup.await.unwrap().unwrap(), None);
    }

    #[tokio::test(start_paused = true)]
    async fn a_store_that_never_answers_times_out() {
        let (mut client, mut http) = spawn_client();
        let topic_id: EntityTopicId = "device/main/service/collectd".parse().unwrap();

        let lookup = tokio::spawn(async move { client.get(&topic_id).await });

        // The request is received, but left unanswered
        http.recv().await.unwrap();

        assert!(matches!(
            lookup.await.unwrap(),
            Err(EntityStoreClientError::Timeout { .. })
        ));
    }

    #[tokio::test]
    async fn any_other_status_is_an_error() {
        let (mut client, mut http) = spawn_client();
        let topic_id: EntityTopicId = "device/main/service/collectd".parse().unwrap();

        let lookup = tokio::spawn(async move { client.get(&topic_id).await });

        http.recv().await.unwrap();
        http.send(HttpResponseBuilder::new().status(500).build())
            .await
            .unwrap();

        assert!(lookup.await.unwrap().is_err());
    }

    fn spawn_client() -> (EntityStoreClient, FakeServerBox<HttpRequest, HttpResult>) {
        let mut http = FakeServerBox::builder();
        let client = EntityStoreClient::new(
            "http://127.0.0.1:8000/te/v1/entities".into(),
            ClientMessageBox::new(&mut http),
        );
        (client, http.build())
    }
}
