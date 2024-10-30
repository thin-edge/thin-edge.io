use crate::credentials::ConstJwtRetriever;
use crate::credentials::HttpHeaderRequest;
use crate::credentials::HttpHeaderResult;
use crate::handle::C8YHttpProxy;
use crate::messages::CreateEvent;
use crate::C8YHttpConfig;
use crate::C8YHttpProxyBuilder;
use async_trait::async_trait;
use c8y_api::json_c8y::C8yEventResponse;
use c8y_api::json_c8y::C8yUpdateSoftwareListResponse;
use c8y_api::json_c8y::InternalIdResponse;
use http::header::AUTHORIZATION;
use http::HeaderMap;
use http::StatusCode;
use mockito::Matcher;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use tedge_actors::test_helpers::FakeServerBox;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::MessageReceiver;
use tedge_actors::Sender;
use tedge_actors::Server;
use tedge_actors::ServerActor;
use tedge_actors::ServerMessageBoxBuilder;
use tedge_config::TEdgeConfigLocation;
use tedge_http_ext::test_helpers::HttpResponseBuilder;
use tedge_http_ext::HttpActor;
use tedge_http_ext::HttpRequest;
use tedge_http_ext::HttpRequestBuilder;
use tedge_http_ext::HttpResult;
use tedge_test_utils::fs::TempTedgeDir;
use time::macros::datetime;

const JWT_TOKEN: &str = "JWT token";
const BEARER_AUTH: &str = "Bearer JWT token";

#[tokio::test]
async fn c8y_http_proxy_requests_the_device_internal_id_on_start() {
    let c8y_host = "c8y.tenant.io";
    let device_id = "device-001";
    let external_id = "external-device-001";
    let tmp_dir = "/tmp";

    let (mut proxy, mut c8y) =
        spawn_c8y_http_proxy(c8y_host.into(), device_id.into(), tmp_dir.into(), JWT_TOKEN).await;

    // Even before any request is sent to the c8y_proxy
    // the proxy requests over HTTP the internal device id.
    let init_request = HttpRequestBuilder::get(format!(
        "https://{c8y_host}/identity/externalIds/c8y_Serial/{device_id}"
    ))
    .headers(&get_test_auth_header(BEARER_AUTH))
    .build()
    .unwrap();
    assert_recv(&mut c8y, Some(init_request)).await;

    // Cumulocity returns the internal device id
    let c8y_response = HttpResponseBuilder::new()
        .status(200)
        .json(&InternalIdResponse::new(device_id, external_id))
        .build()
        .unwrap();
    c8y.send(Ok(c8y_response)).await.unwrap();

    // This internal id is then used by the proxy for subsequent requests.
    // For instance, if the proxy upload a log file
    tokio::spawn(async move {
        // NOTE: this is done in the background because this call awaits for the response.
        proxy
            .upload_log_binary("test.log", "some log content", "device-001".into())
            .await
            .unwrap();
    });

    // then the upload request received by c8y is related to the internal id
    assert_recv(
        &mut c8y,
        Some(
            HttpRequestBuilder::post(format!("https://{c8y_host}/event/events/"))
                .headers(&get_test_auth_header(BEARER_AUTH))
                .header("content-type", "application/json")
                .header("accept", "application/json")
                .build()
                .unwrap(),
        ),
    )
    .await;
}

#[tokio::test]
async fn retry_internal_id_on_expired_jwt() {
    let c8y_host = "c8y.tenant.io";
    let device_id = "device-001";
    let external_id = "external-device-001";
    let tmp_dir = "/tmp";

    let (mut proxy, mut c8y) =
        spawn_c8y_http_proxy(c8y_host.into(), device_id.into(), tmp_dir.into(), JWT_TOKEN).await;

    // Even before any request is sent to the c8y_proxy
    // the proxy requests over HTTP the internal device id.
    let init_request = HttpRequestBuilder::get(format!(
        "https://{c8y_host}/identity/externalIds/c8y_Serial/{device_id}"
    ))
    .headers(&get_test_auth_header(BEARER_AUTH))
    .build()
    .unwrap();
    assert_recv(&mut c8y, Some(init_request)).await;

    // Cumulocity returns unauthorized error (401), because the jwt token has expired
    let c8y_response = HttpResponseBuilder::new().status(401).build().unwrap();
    c8y.send(Ok(c8y_response)).await.unwrap();
    assert_recv(
        &mut c8y,
        Some(
            HttpRequestBuilder::get(format!(
                "https://{c8y_host}/identity/externalIds/c8y_Serial/{device_id}"
            ))
            .headers(&get_test_auth_header(BEARER_AUTH))
            .build()
            .unwrap(),
        ),
    )
    .await;
    // Mapper retries to get the internal device id, after getting a fresh jwt token
    let c8y_response = HttpResponseBuilder::new()
        .status(200)
        .json(&InternalIdResponse::new(device_id, external_id))
        .build()
        .unwrap();
    c8y.send(Ok(c8y_response)).await.unwrap();

    // This internal id is then used by the proxy for subsequent requests.
    // For instance, if the proxy upload a log file
    tokio::spawn(async move {
        // NOTE: this is done in the background because this call awaits for the response.
        proxy
            .upload_log_binary("test.log", "some log content", "device-001".into())
            .await
            .unwrap();
    });

    // then the upload request received by c8y is related to the internal id
    assert_recv(
        &mut c8y,
        Some(
            HttpRequestBuilder::post(format!("https://{c8y_host}/event/events/"))
                .headers(&get_test_auth_header(BEARER_AUTH))
                .header("content-type", "application/json")
                .header("accept", "application/json")
                .build()
                .unwrap(),
        ),
    )
    .await;
}

#[tokio::test]
async fn retry_get_internal_id_when_not_found() {
    let c8y_host = "c8y.tenant.io";
    let main_device_id = "device-001";
    let tmp_dir = "/tmp";
    let child_device_id = "child-101";

    let (mut proxy, mut c8y) = spawn_c8y_http_proxy(
        c8y_host.into(),
        main_device_id.into(),
        tmp_dir.into(),
        JWT_TOKEN,
    )
    .await;

    // Mock server definition
    tokio::spawn(async move {
        // Respond to the initial get_id request for the main device
        let get_internal_id_url =
            format!("https://{c8y_host}/identity/externalIds/c8y_Serial/{main_device_id}");
        let init_request = HttpRequestBuilder::get(get_internal_id_url)
            .headers(&get_test_auth_header(BEARER_AUTH))
            .build()
            .unwrap();
        assert_recv(&mut c8y, Some(init_request)).await;
        let c8y_response = HttpResponseBuilder::new()
            .status(200)
            .json(&InternalIdResponse::new("100", main_device_id))
            .build()
            .unwrap();
        c8y.send(Ok(c8y_response)).await.unwrap();

        let get_internal_id_url =
            format!("https://{c8y_host}/identity/externalIds/c8y_Serial/{child_device_id}");

        // Fail the first 2 internal id lookups for the child device
        for _ in 0..2 {
            assert_recv(
                &mut c8y,
                Some(
                    HttpRequestBuilder::get(&get_internal_id_url)
                        .headers(&get_test_auth_header(BEARER_AUTH))
                        .build()
                        .unwrap(),
                ),
            )
            .await;
            let c8y_response = HttpResponseBuilder::new()
                .status(StatusCode::NOT_FOUND)
                .build()
                .unwrap();
            c8y.send(Ok(c8y_response)).await.unwrap();
        }

        // Let the next get_id request succeed
        assert_recv(
            &mut c8y,
            Some(
                HttpRequestBuilder::get(&get_internal_id_url)
                    .headers(&get_test_auth_header(BEARER_AUTH))
                    .build()
                    .unwrap(),
            ),
        )
        .await;
        let c8y_response = HttpResponseBuilder::new()
            .status(200)
            .json(&InternalIdResponse::new("200", child_device_id))
            .build()
            .unwrap();
        c8y.send(Ok(c8y_response)).await.unwrap();

        // Then let the software_list update succeed
        let c8y_software_list = C8yUpdateSoftwareListResponse::default();
        assert_recv(
            &mut c8y,
            Some(
                HttpRequestBuilder::put(format!("https://{c8y_host}/inventory/managedObjects/200"))
                    .header("content-type", "application/json")
                    .header("accept", "application/json")
                    .headers(&get_test_auth_header(BEARER_AUTH))
                    .json(&c8y_software_list)
                    .build()
                    .unwrap(),
            ),
        )
        .await;
        let c8y_response = HttpResponseBuilder::new().status(200).build().unwrap();
        c8y.send(Ok(c8y_response)).await.unwrap();
    });

    let res = proxy
        .send_software_list_http(
            C8yUpdateSoftwareListResponse::default(),
            child_device_id.into(),
        )
        .await;
    assert!(res.is_ok(), "Expected software list request to succeed");
}

#[tokio::test]
async fn get_internal_id_retry_fails_after_exceeding_attempts_threshold() {
    let c8y_host = "c8y.tenant.io";
    let main_device_id = "device-001";
    let tmp_dir = "/tmp";
    let child_device_id = "child-101";

    let (mut proxy, mut c8y) = spawn_c8y_http_proxy(
        c8y_host.into(),
        main_device_id.into(),
        tmp_dir.into(),
        JWT_TOKEN,
    )
    .await;

    // Mock server definition
    tokio::spawn(async move {
        // On receipt of the initial get_id request for the main device...
        let get_internal_id_url =
            format!("https://{c8y_host}/identity/externalIds/c8y_Serial/{main_device_id}");
        let init_request = HttpRequestBuilder::get(get_internal_id_url)
            .headers(&get_test_auth_header(BEARER_AUTH))
            .build()
            .unwrap();
        assert_recv(&mut c8y, Some(init_request)).await;
        // ...respond with its internal id
        let c8y_response = HttpResponseBuilder::new()
            .status(200)
            .json(&InternalIdResponse::new("100", main_device_id))
            .build()
            .unwrap();
        c8y.send(Ok(c8y_response)).await.unwrap();

        // Always fail the internal id lookup for the child device
        loop {
            let get_internal_id_url =
                format!("https://{c8y_host}/identity/externalIds/c8y_Serial/{child_device_id}");
            assert_recv(
                &mut c8y,
                Some(
                    HttpRequestBuilder::get(&get_internal_id_url)
                        .headers(&get_test_auth_header(BEARER_AUTH))
                        .build()
                        .unwrap(),
                ),
            )
            .await;
            let c8y_response = HttpResponseBuilder::new()
                .status(StatusCode::NOT_FOUND)
                .build()
                .unwrap();
            c8y.send(Ok(c8y_response)).await.unwrap();
        }
    });

    // Fetch the software list so that it internally invokes get_internal_id
    let res = proxy
        .send_software_list_http(
            C8yUpdateSoftwareListResponse::default(),
            child_device_id.into(),
        )
        .await;
    assert!(res.is_err(), "Expected software list request to succeed");
}

#[tokio::test]
async fn retry_internal_id_on_expired_jwt_with_mock() {
    let external_id = "device-001";
    let tmp_dir = "/tmp";
    let internal_id = "internal-device-001";

    let response = InternalIdResponse::new(internal_id, external_id);
    let response = serde_json::to_string(&response).unwrap();
    // Start a lightweight mock server.
    let mut server = mockito::Server::new_async().await;

    let _mock1 = server
        .mock("GET", "/identity/externalIds/c8y_Serial/device-001")
        .match_header(
            "Authorization",
            Matcher::Exact("Bearer Cached JWT token".into()),
        )
        .with_status(401)
        .create_async()
        .await;
    let _mock2 = server
        .mock("GET", "/identity/externalIds/c8y_Serial/device-001")
        .match_header(
            "Authorization",
            Matcher::Exact("Bearer Fresh JWT token".into()),
        )
        .with_status(200)
        .with_body(response)
        .create_async()
        .await;

    let target_url = server.url();
    let mut auth = ServerMessageBoxBuilder::new("Auth Actor", 16);

    let ttd = TempTedgeDir::new();
    let config_loc = TEdgeConfigLocation::from_custom_root(ttd.path());
    let tedge_config = config_loc.load().unwrap();
    let mut http_actor = HttpActor::new(&tedge_config).builder();

    let config = C8YHttpConfig {
        c8y_http_host: target_url.clone(),
        c8y_mqtt_host: target_url.clone(),
        device_id: external_id.into(),
        tmp_dir: tmp_dir.into(),
        retry_interval: Duration::from_millis(100),
    };
    let c8y_proxy_actor = C8YHttpProxyBuilder::new(config, &mut http_actor, &mut auth);
    let jwt_actor = ServerActor::new(DynamicJwtRetriever { count: 0 }, auth.build());

    tokio::spawn(async move { http_actor.run().await });
    tokio::spawn(async move { jwt_actor.run().await });
    let mut proxy = c8y_proxy_actor.build();

    let result = proxy.try_get_internal_id(external_id.into()).await;
    assert_eq!(internal_id, result.unwrap());
}

#[tokio::test]
async fn retry_create_event_on_expired_jwt_with_mock() {
    let external_id = "device-001";
    let tmp_dir = "/tmp";
    let internal_id = "12345678";
    let event_id = "87654321";

    let event = CreateEvent {
        event_type: "click_event".into(),
        time: datetime!(2021-04-23 19:00:00 +05:00),
        text: "Someone clicked".into(),
        extras: HashMap::new(),
        device_id: external_id.to_string(),
    };

    let response = C8yEventResponse {
        id: event_id.to_string(),
    };
    let response = serde_json::to_string(&response).unwrap();
    // Start a lightweight mock server.
    let mut server = mockito::Server::new_async().await;

    let _mock1 = server
        .mock("POST", "/event/events/")
        .match_header(
            "authorization",
            Matcher::Exact("Bearer Cached JWT Token".into()),
        )
        .with_status(401)
        .create_async()
        .await;

    let _mock2 = server
        .mock("POST", "/event/events/")
        .match_header(
            "authorization",
            Matcher::Exact("Bearer Fresh JWT token".into()),
        )
        .with_status(200)
        .with_body(response)
        .create_async()
        .await;

    let target_url = server.url();
    let mut jwt = ServerMessageBoxBuilder::new("JWT Actor", 16);

    let ttd = TempTedgeDir::new();
    let config_loc = TEdgeConfigLocation::from_custom_root(ttd.path());
    let tedge_config = config_loc.load().unwrap();
    let mut http_actor = HttpActor::new(&tedge_config).builder();

    let config = C8YHttpConfig {
        c8y_http_host: target_url.clone(),
        c8y_mqtt_host: target_url.clone(),
        device_id: external_id.into(),
        tmp_dir: tmp_dir.into(),
        retry_interval: Duration::from_millis(100),
    };
    let c8y_proxy_actor = C8YHttpProxyBuilder::new(config, &mut http_actor, &mut jwt);
    let jwt_actor = ServerActor::new(DynamicJwtRetriever { count: 1 }, jwt.build());

    tokio::spawn(async move { http_actor.run().await });
    tokio::spawn(async move { jwt_actor.run().await });
    let mut proxy = c8y_proxy_actor.build();
    // initialize the endpoint for mocking purpose
    proxy.end_point.device_id = external_id.into();
    proxy
        .end_point
        .set_internal_id(external_id.into(), internal_id.into());
    let headers = get_test_auth_header("Bearer Cached JWT Token");
    proxy.end_point.headers = headers;

    let result = proxy.create_event(event).await;
    assert_eq!(event_id, result.unwrap());
}

#[tokio::test]
async fn retry_software_list_once_with_fresh_internal_id() {
    let c8y_host = "c8y.tenant.io";
    let device_id = "device-001";
    let external_id = "external-device-001";
    let tmp_dir = "/tmp";

    let (mut proxy, mut c8y) =
        spawn_c8y_http_proxy(c8y_host.into(), device_id.into(), tmp_dir.into(), JWT_TOKEN).await;

    // Even before any request is sent to the c8y_proxy
    // the proxy requests over HTTP the internal device id.
    let _init_request = HttpRequestBuilder::get(format!(
        "https://{c8y_host}/identity/externalIds/c8y_Serial/{device_id}"
    ))
    .headers(&get_test_auth_header(BEARER_AUTH))
    .build()
    .unwrap();
    // skip the message
    c8y.recv().await;

    // Cumulocity returns the internal device id
    let c8y_response = HttpResponseBuilder::new()
        .status(200)
        .json(&InternalIdResponse::new(device_id, external_id))
        .build()
        .unwrap();
    c8y.send(Ok(c8y_response)).await.unwrap();

    // This internal id is then used by the proxy for subsequent requests.
    // Create  the  software list and publish
    let c8y_software_list = C8yUpdateSoftwareListResponse::default();

    tokio::spawn(async move {
        // NOTE: this is done in the background because this call awaits for the response.
        proxy
            .send_software_list_http(c8y_software_list, device_id.into())
            .await
    });

    let c8y_software_list = C8yUpdateSoftwareListResponse::default();
    // then the upload request received by c8y is related to the internal id
    assert_recv(
        &mut c8y,
        Some(
            HttpRequestBuilder::put(format!(
                "https://{c8y_host}/inventory/managedObjects/{device_id}"
            ))
            .header("content-type", "application/json")
            .header("accept", "application/json")
            .headers(&get_test_auth_header(BEARER_AUTH))
            .json(&c8y_software_list)
            .build()
            .unwrap(),
        ),
    )
    .await;

    // The software list upload fails because the device identified with internal id not found
    let c8y_response = HttpResponseBuilder::new()
        .status(404)
        .json(&InternalIdResponse::new(device_id, external_id))
        .build()
        .unwrap();
    c8y.send(Ok(c8y_response)).await.unwrap();

    // Now the mapper gets a new internal id for the specific device id
    assert_recv(
        &mut c8y,
        Some(
            HttpRequestBuilder::get(format!(
                "https://{c8y_host}/identity/externalIds/c8y_Serial/{device_id}"
            ))
            .headers(&get_test_auth_header(BEARER_AUTH))
            .build()
            .unwrap(),
        ),
    )
    .await;

    // Cumulocity returns the internal device id, after retrying with the fresh jwt token
    let c8y_response = HttpResponseBuilder::new()
        .status(200)
        .json(&InternalIdResponse::new(device_id, external_id))
        .build()
        .unwrap();
    c8y.send(Ok(c8y_response)).await.unwrap();

    let c8y_software_list = C8yUpdateSoftwareListResponse::default();
    // then the upload request received by c8y is related to the internal id
    assert_recv(
        &mut c8y,
        Some(
            HttpRequestBuilder::put(format!(
                "https://{c8y_host}/inventory/managedObjects/{device_id}"
            ))
            .headers(&get_test_auth_header(BEARER_AUTH))
            .header("content-type", "application/json")
            .header("accept", "application/json")
            .json(&c8y_software_list)
            .build()
            .unwrap(),
        ),
    )
    .await;
}

#[tokio::test]
async fn auto_retry_upload_log_binary_when_internal_id_expires() {
    let c8y_host = "c8y.tenant.io";
    let device_id = "device-001";
    let external_id = "external-device-001";
    let tmp_dir = "/tmp";

    let (mut proxy, mut c8y) =
        spawn_c8y_http_proxy(c8y_host.into(), device_id.into(), tmp_dir.into(), JWT_TOKEN).await;

    // Even before any request is sent to the c8y_proxy
    // the proxy requests over HTTP the internal device id.
    let init_request = HttpRequestBuilder::get(format!(
        "https://{c8y_host}/identity/externalIds/c8y_Serial/{device_id}"
    ))
    .headers(&get_test_auth_header(BEARER_AUTH))
    .build()
    .unwrap();
    assert_recv(&mut c8y, Some(init_request)).await;
    // Cumulocity returns the internal device id
    let c8y_response = HttpResponseBuilder::new()
        .status(200)
        .json(&InternalIdResponse::new(device_id, external_id))
        .build()
        .unwrap();
    c8y.send(Ok(c8y_response)).await.unwrap();
    // This internal id is then used by the proxy for subsequent requests.
    // For instance, if the proxy upload a log file
    tokio::spawn(async move {
        // NOTE: this is done in the background because this call awaits for the response.
        proxy
            .upload_log_binary("test.log", "some log content", "device-001".into())
            .await
            .unwrap();
    });
    // then the upload request received by c8y is related to the internal id
    assert_recv(
        &mut c8y,
        Some(
            HttpRequestBuilder::post(format!("https://{c8y_host}/event/events/"))
                .headers(&get_test_auth_header(BEARER_AUTH))
                .header("content-type", "application/json")
                .header("accept", "application/json")
                .build()
                .unwrap(),
        ),
    )
    .await;

    // Creating the event over http failed due to the device is NOT_FOUND
    let c8y_response = HttpResponseBuilder::new()
        .status(404)
        .json(&InternalIdResponse::new(device_id, external_id))
        .build()
        .unwrap();
    c8y.send(Ok(c8y_response)).await.unwrap();

    // Mapper retries the call with a request to get the internal id
    assert_recv(
        &mut c8y,
        Some(
            HttpRequestBuilder::get(format!(
                "https://{c8y_host}/identity/externalIds/c8y_Serial/{device_id}"
            ))
            .headers(&get_test_auth_header(BEARER_AUTH))
            .build()
            .unwrap(),
        ),
    )
    .await;

    let c8y_response = HttpResponseBuilder::new()
        .status(200)
        .json(&InternalIdResponse::new(device_id, external_id))
        .build()
        .unwrap();
    c8y.send(Ok(c8y_response)).await.unwrap();

    // then the upload request received by c8y is related to the internal id
    assert_recv(
        &mut c8y,
        Some(
            HttpRequestBuilder::post(format!("https://{c8y_host}/event/events/"))
                .headers(&get_test_auth_header(BEARER_AUTH))
                .header("content-type", "application/json")
                .header("accept", "application/json")
                .build()
                .unwrap(),
        ),
    )
    .await;
}

/// Spawn an `C8YHttpProxyActor` instance
/// Return two handles:
/// - one `C8YHttpProxy` to send requests to the actor
/// - one `ServerMessageBoxBuilder<HttpRequest,HttpResponse> to fake the behavior of C8Y REST.
///
/// This also spawns an actor to generate fake JWT tokens.
/// The tests will only check that the http requests include this token.
async fn spawn_c8y_http_proxy(
    c8y_host: String,
    device_id: String,
    tmp_dir: PathBuf,
    token: &str,
) -> (C8YHttpProxy, FakeServerBox<HttpRequest, HttpResult>) {
    let mut jwt = ServerMessageBoxBuilder::new("JWT Actor", 16);

    let mut http = FakeServerBox::builder();

    let config = C8YHttpConfig {
        c8y_http_host: c8y_host.clone(),
        c8y_mqtt_host: c8y_host,
        device_id,
        tmp_dir,
        retry_interval: Duration::from_millis(10),
    };
    let mut c8y_proxy_actor = C8YHttpProxyBuilder::new(config, &mut http, &mut jwt);
    let proxy = C8YHttpProxy::new(&mut c8y_proxy_actor);

    let jwt_actor = ServerActor::new(
        ConstJwtRetriever {
            token: token.to_string(),
        },
        jwt.build(),
    );

    tokio::spawn(async move { jwt_actor.run().await });
    tokio::spawn(async move {
        let actor = c8y_proxy_actor.build();
        let _ = actor.run().await;
    });

    (proxy, http.build())
}

pub(crate) struct DynamicJwtRetriever {
    pub count: usize,
}

#[async_trait]
impl Server for DynamicJwtRetriever {
    type Request = HttpHeaderRequest;
    type Response = HttpHeaderResult;

    fn name(&self) -> &str {
        "DynamicJwtRetriever"
    }

    async fn handle(&mut self, _request: Self::Request) -> Self::Response {
        let mut headers = HeaderMap::new();
        if self.count == 0 {
            self.count += 1;
            headers.insert(AUTHORIZATION, "Bearer Cached JWT token".parse().unwrap());
        } else {
            headers.insert(AUTHORIZATION, "Bearer Fresh JWT token".parse().unwrap());
        }
        Ok(headers)
    }
}

fn get_test_auth_header(value: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(AUTHORIZATION, value.parse().unwrap());
    headers
}

async fn assert_recv(
    from: &mut FakeServerBox<HttpRequest, HttpResult>,
    expected: Option<HttpRequest>,
) {
    let actual = from.recv().await;
    tedge_http_ext::test_helpers::assert_request_eq(actual, expected)
}
