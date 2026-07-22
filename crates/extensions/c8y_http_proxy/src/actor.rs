use crate::messages::C8YRestError;
use crate::messages::CreateEvent;
use crate::messages::EventId;
use crate::messages::SoftwareListResponse;
use crate::C8YHttpConfig;
use c8y_api::http_proxy::C8yEndPoint;
use c8y_api::json_c8y::C8yCreateEvent;
use c8y_api::json_c8y::C8yEventResponse;
use c8y_api::json_c8y::C8yManagedObject;
use c8y_api::json_c8y::InternalIdResponse;
use http::StatusCode;
use serde_json::json;
use std::time::Duration;
use tedge_actors::ClientMessageBox;
use tedge_http_ext::HttpError;
use tedge_http_ext::HttpRequest;
use tedge_http_ext::HttpRequestBuilder;
use tedge_http_ext::HttpResponseExt;
use tedge_http_ext::HttpResult;
use tokio::time::sleep;
use tracing::debug;

/// Maximum number of attempts to fetch the Cumulocity internal id of an entity.
///
/// A freshly registered device (typically a child device) may not have been created yet in the
/// Cumulocity inventory when the mapper first needs its internal id (e.g. to publish its software
/// list). In that case the lookup transiently fails with `404 Not Found`. Retrying a few times
/// with a growing delay lets the mapper recover from this race instead of silently dropping the
/// request.
const INTERNAL_ID_MAX_ATTEMPTS: u32 = 8;

/// Delay before the first internal-id lookup retry (doubles after each attempt).
const INTERNAL_ID_RETRY_INITIAL_INTERVAL: Duration = Duration::from_millis(1000);

/// Upper bound for the delay between internal-id lookup retries.
const INTERNAL_ID_RETRY_MAX_INTERVAL: Duration = Duration::from_secs(10);

#[derive(Clone)]
pub struct C8YHttpProxyActor {
    pub(crate) end_point: C8yEndPoint,

    /// Connection to an HTTP actor
    pub(crate) http: ClientMessageBox<HttpRequest, HttpResult>,
}

impl C8YHttpProxyActor {
    pub fn new(config: C8YHttpConfig, http: ClientMessageBox<HttpRequest, HttpResult>) -> Self {
        let end_point =
            C8yEndPoint::new(&config.c8y_http_host, &config.c8y_mqtt_host, config.proxy);
        C8YHttpProxyActor { end_point, http }
    }

    pub async fn try_get_internal_id(&mut self, device_id: &str) -> Result<String, C8YRestError> {
        let url_get_id: String = self.end_point.proxy_url_for_internal_id(device_id);

        let mut attempt = 0;
        let mut interval = INTERNAL_ID_RETRY_INITIAL_INTERVAL;
        loop {
            attempt += 1;
            let request = HttpRequestBuilder::get(&url_get_id).build()?;
            match self.http.await_response(request).await?.error_for_status() {
                Ok(response) => {
                    let internal_id_response: InternalIdResponse = response.json().await?;
                    return Ok(internal_id_response.id());
                }
                // The entity is registered locally but not created yet in the c8y inventory:
                // retry the lookup a few times before giving up so the request is not lost.
                Err(err)
                    if is_entity_not_created_yet(&err) && attempt < INTERNAL_ID_MAX_ATTEMPTS =>
                {
                    debug!(
                        "Internal id of {device_id} not available yet \
                         (attempt {attempt}/{INTERNAL_ID_MAX_ATTEMPTS}), retrying in {interval:?}"
                    );
                    sleep(interval).await;
                    interval = (interval * 2).min(INTERNAL_ID_RETRY_MAX_INTERVAL);
                }
                Err(err) => return Err(err.into()),
            }
        }
    }

    pub(crate) async fn create_event(
        &mut self,
        c8y_event: CreateEvent,
    ) -> Result<EventId, C8YRestError> {
        let internal_id = self.try_get_internal_id(&c8y_event.device_id).await?;
        let updated_c8y_event = C8yCreateEvent {
            source: Some(C8yManagedObject { id: internal_id }),
            event_type: c8y_event.event_type,
            time: c8y_event.time,
            text: c8y_event.text,
            extras: c8y_event.extras,
        };
        let create_event_url = self.end_point.proxy_url_for_create_event();
        let request = HttpRequestBuilder::post(create_event_url)
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .json(&updated_c8y_event)
            .build()?;

        let http_result = self.http.await_response(request).await?;
        let http_response = http_result.error_for_status()?;
        let event_response: C8yEventResponse = http_response.json().await?;
        Ok(event_response.id)
    }

    pub(crate) async fn send_software_list_http(
        &mut self,
        software_list: SoftwareListResponse,
    ) -> Result<(), C8YRestError> {
        let c8y_internal_id = self.try_get_internal_id(&software_list.device_id).await?;
        let url = self.end_point.proxy_url_for_sw_list(&c8y_internal_id);
        let request = HttpRequestBuilder::put(url)
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .json(&software_list.c8y_software_list)
            .build()?;

        let http_result = self.http.await_response(request).await?;
        let _ = http_result.error_for_status()?;
        Ok(())
    }

    pub(crate) async fn update_child_device_parent(
        &mut self,
        device_xid: &str,
        old_parent_xid: &str,
        new_parent_xid: &str,
    ) -> Result<(), C8YRestError> {
        let device_id = self.try_get_internal_id(device_xid).await?;
        let old_parent_id = self.try_get_internal_id(old_parent_xid).await?;
        let new_parent_id = self.try_get_internal_id(new_parent_xid).await?;

        let url = self
            .end_point
            .proxy_url_for_assign_child_device_to_parent(&new_parent_id);
        let payload = json!({
            "managedObject": {
                "id": device_id
            }
        });
        let request = HttpRequestBuilder::post(url)
            .header("Content-Type", "application/json")
            .json(&payload)
            .build()?;
        let http_result = self.http.await_response(request).await?;
        let _ = http_result.error_for_status()?;

        let url = self
            .end_point
            .proxy_url_for_remove_child_device_from_parent(&old_parent_id, &device_id);
        let request = HttpRequestBuilder::delete(url).build()?;
        let http_result = self.http.await_response(request).await?;
        let _ = http_result.error_for_status()?;

        Ok(())
    }

    pub(crate) async fn update_child_addition_parent(
        &mut self,
        service_xid: &str,
        old_parent_xid: &str,
        new_parent_xid: &str,
    ) -> Result<(), C8YRestError> {
        let child_id = self.try_get_internal_id(service_xid).await?;
        let old_parent_id = self.try_get_internal_id(old_parent_xid).await?;
        let new_parent_id = self.try_get_internal_id(new_parent_xid).await?;

        let url = self
            .end_point
            .proxy_url_for_assign_child_addition_to_parent(&new_parent_id);
        let payload = json!({
            "managedObject": {
                "id": child_id
            }
        });
        let request = HttpRequestBuilder::post(url)
            .header("Content-Type", "application/json")
            .json(&payload)
            .build()?;
        let http_result = self.http.await_response(request).await?;
        let _ = http_result.error_for_status()?;

        let url = self
            .end_point
            .proxy_url_for_remove_child_addition_from_parent(&old_parent_id, &child_id);
        let request = HttpRequestBuilder::delete(url).build()?;
        let http_result = self.http.await_response(request).await?;
        let _ = http_result.error_for_status()?;

        Ok(())
    }
}

/// Whether an HTTP error indicates that a c8y entity has not been created yet.
///
/// The internal-id lookup of an entity that is registered locally but not created yet in the
/// Cumulocity inventory returns `404 Not Found`. This is a transient condition worth retrying.
fn is_entity_not_created_yet(err: &HttpError) -> bool {
    matches!(
        err,
        HttpError::HttpStatusError { code, .. } if *code == StatusCode::NOT_FOUND
    )
}
