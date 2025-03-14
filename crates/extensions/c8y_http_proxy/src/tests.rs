use crate::handle::C8YHttpProxy;
use crate::messages::CreateEvent;
use crate::C8YHttpConfig;
use c8y_api::json_c8y::C8yEventResponse;
use c8y_api::json_c8y::C8yUpdateSoftwareListResponse;
use c8y_api::json_c8y::InternalIdResponse;
use c8y_api::proxy_url::Protocol;
use c8y_api::proxy_url::ProxyUrlGenerator;
use http::StatusCode;
use std::collections::HashMap;
use tedge_actors::test_helpers::FakeServerBox;
use tedge_actors::Builder;
use tedge_actors::MessageReceiver;
use tedge_actors::Sender;
use tedge_config::TEdgeConfigLocation;
use tedge_http_ext::test_helpers::HttpResponseBuilder;
use tedge_http_ext::HttpActor;
use tedge_http_ext::HttpRequest;
use tedge_http_ext::HttpRequestBuilder;
use tedge_http_ext::HttpResult;
use tedge_test_utils::fs::TempTedgeDir;
use time::macros::datetime;

#[tokio::test]
async fn c8y_http_proxy_requests_the_device_internal_id_on_start() {
    let c8y_host = "c8y.tenant.io";
    let device_id = "device-001";
    let external_id = "external-device-001";

    let (mut proxy, mut c8y) = spawn_c8y_http_proxy(c8y_host.into(), device_id.into()).await;

    tokio::spawn(async move {
        proxy.c8y_internal_id(device_id).await.unwrap();
    });

    // On start the proxy requests over HTTP the internal device id.
    let init_request = HttpRequestBuilder::get(format!(
        "http://localhost:8001/c8y/identity/externalIds/c8y_Serial/{device_id}"
    ))
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
}

#[tokio::test]
async fn get_internal_id() {
    let c8y_host = "c8y.tenant.io";
    let device_id = "device-001";
    let external_id = "external-device-001";

    let (mut proxy, mut c8y) = spawn_c8y_http_proxy(c8y_host.into(), device_id.into()).await;

    tokio::spawn(async move {
        proxy.c8y_internal_id(device_id).await.unwrap();
    });

    // the proxy requests over HTTP the internal device id.
    assert_recv(
        &mut c8y,
        Some(
            HttpRequestBuilder::get(format!(
                "http://localhost:8001/c8y/identity/externalIds/c8y_Serial/{device_id}"
            ))
            .build()
            .unwrap(),
        ),
    )
    .await;

    // C8Y send back the internal id
    let c8y_response = HttpResponseBuilder::new()
        .status(200)
        .json(&InternalIdResponse::new(device_id, external_id))
        .build()
        .unwrap();
    c8y.send(Ok(c8y_response)).await.unwrap();
}

#[tokio::test]
async fn get_internal_id_before_posting_software_list() {
    let c8y_host = "c8y.tenant.io";
    let main_device_id = "device-001";
    let child_device_id = "child-101";

    let (mut proxy, mut c8y) = spawn_c8y_http_proxy(c8y_host.into(), main_device_id.into()).await;

    // Mock server definition
    tokio::spawn(async move {
        let get_internal_id_url =
            format!("http://localhost:8001/c8y/identity/externalIds/c8y_Serial/{child_device_id}");

        // Let the next get_id request succeed
        assert_recv(
            &mut c8y,
            Some(
                HttpRequestBuilder::get(&get_internal_id_url)
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
                HttpRequestBuilder::put(
                    "http://localhost:8001/c8y/inventory/managedObjects/200".to_string(),
                )
                .header("content-type", "application/json")
                .header("accept", "application/json")
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
    let child_device_id = "child-101";

    let (mut proxy, mut c8y) = spawn_c8y_http_proxy(c8y_host.into(), main_device_id.into()).await;

    // Mock server definition
    tokio::spawn(async move {
        // Always fail the internal id lookup for the child device
        loop {
            let get_internal_id_url = format!(
                "http://localhost:8001/c8y/identity/externalIds/c8y_Serial/{child_device_id}"
            );
            assert_recv(
                &mut c8y,
                Some(
                    HttpRequestBuilder::get(&get_internal_id_url)
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
async fn get_internal_id_with_mock() {
    let external_id = "device-001";
    let internal_id = "internal-device-001";

    let response = InternalIdResponse::new(internal_id, external_id);
    let response = serde_json::to_string(&response).unwrap();
    // Start a lightweight mock server.
    let mut server = mockito::Server::new_async().await;

    let _mock1 = server
        .mock("GET", "/c8y/identity/externalIds/c8y_Serial/device-001")
        .with_status(200)
        .with_body(response)
        .create_async()
        .await;

    let target_url = "remote.c8y.com".to_string();
    let server_url = server.host_with_port();
    let (proxy_host, proxy_port) = server_url.split_once(':').unwrap();
    let proxy = ProxyUrlGenerator::new(
        proxy_host.into(),
        proxy_port.parse().unwrap(),
        Protocol::Http,
    );

    let ttd = TempTedgeDir::new();
    let config_loc = TEdgeConfigLocation::from_custom_root(ttd.path());
    let tedge_config = config_loc.load().await.unwrap();
    let tls_config = tedge_config.http.client_tls_config().unwrap();
    let mut http_actor = HttpActor::new(tls_config).builder();

    let config = C8YHttpConfig {
        c8y_http_host: target_url.clone(),
        c8y_mqtt_host: target_url.clone(),
        device_id: external_id.into(),
        proxy,
    };
    let mut proxy = C8YHttpProxy::new(config, &mut http_actor);

    tokio::spawn(async move { http_actor.run().await });

    let result = proxy.c8y_internal_id(external_id).await;
    assert_eq!(internal_id, result.unwrap());
}

#[tokio::test]
async fn request_internal_id_before_posting_new_event() {
    let external_id = "device-001";
    let internal_id = "12345678";
    let event_id = "87654321";

    let c8y_serial = InternalIdResponse::new(internal_id, external_id);
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

    let _mock0 = server
        .mock("GET", "/c8y/identity/externalIds/c8y_Serial/device-001")
        .with_status(200)
        .with_body(serde_json::to_string(&c8y_serial).unwrap())
        .create_async()
        .await;

    let _mock2 = server
        .mock("POST", "/c8y/event/events/")
        .with_status(200)
        .with_body(response)
        .create_async()
        .await;

    let target_url = "remote.c8y.com".to_string();
    let server_url = server.host_with_port();
    let (proxy_host, proxy_port) = server_url.split_once(':').unwrap();
    let proxy = ProxyUrlGenerator::new(
        proxy_host.into(),
        proxy_port.parse().unwrap(),
        Protocol::Http,
    );

    let ttd = TempTedgeDir::new();
    let config_loc = TEdgeConfigLocation::from_custom_root(ttd.path());
    let tedge_config = config_loc.load().await.unwrap();
    let tls_config = tedge_config.http.client_tls_config().unwrap();
    let mut http_actor = HttpActor::new(tls_config).builder();

    let config = C8YHttpConfig {
        c8y_http_host: target_url.clone(),
        c8y_mqtt_host: target_url.clone(),
        device_id: external_id.into(),
        proxy,
    };
    let mut proxy = C8YHttpProxy::new(config, &mut http_actor);

    tokio::spawn(async move { http_actor.run().await });

    let result = proxy.send_event(event).await;
    assert_eq!(event_id, result.unwrap());
}

#[tokio::test]
async fn request_internal_id_before_posting_software_list() {
    let c8y_host = "c8y.tenant.io";
    let device_id = "device-001";
    let external_id = "external-device-001";

    let (mut proxy, mut c8y) = spawn_c8y_http_proxy(c8y_host.into(), device_id.into()).await;
    // Create  the  software list and publish
    let c8y_software_list = C8yUpdateSoftwareListResponse::default();

    tokio::spawn(async move {
        // NOTE: this is done in the background because this call awaits for the response.
        proxy
            .send_software_list_http(c8y_software_list, device_id.into())
            .await
    });

    // The proxy requests over HTTP the internal device id.
    assert_recv(
        &mut c8y,
        Some(
            HttpRequestBuilder::get(format!(
                "http://localhost:8001/c8y/identity/externalIds/c8y_Serial/{device_id}"
            ))
            .build()
            .unwrap(),
        ),
    )
    .await;

    // Cumulocity returns the internal device id
    let c8y_response = HttpResponseBuilder::new()
        .status(200)
        .json(&InternalIdResponse::new(device_id, external_id))
        .build()
        .unwrap();
    c8y.send(Ok(c8y_response)).await.unwrap();

    // This internal id is then used by the proxy for subsequent requests.

    let c8y_software_list = C8yUpdateSoftwareListResponse::default();
    // then the upload request received by c8y is related to the internal id
    assert_recv(
        &mut c8y,
        Some(
            HttpRequestBuilder::put(format!(
                "http://localhost:8001/c8y/inventory/managedObjects/{device_id}"
            ))
            .header("content-type", "application/json")
            .header("accept", "application/json")
            .json(&c8y_software_list)
            .build()
            .unwrap(),
        ),
    )
    .await;
}

/// Return two handles:
/// - one `C8YHttpProxy` to send HTTP requests to C8Y
/// - one `ServerMessageBoxBuilder<HttpRequest,HttpResponse> to fake the behavior of C8Y REST.
async fn spawn_c8y_http_proxy(
    c8y_host: String,
    device_id: String,
) -> (C8YHttpProxy, FakeServerBox<HttpRequest, HttpResult>) {
    let mut http = FakeServerBox::builder();

    let config = C8YHttpConfig {
        c8y_http_host: c8y_host.clone(),
        c8y_mqtt_host: c8y_host,
        device_id,
        proxy: ProxyUrlGenerator::default(),
    };
    let proxy = C8YHttpProxy::new(config, &mut http);

    (proxy, http.build())
}

async fn assert_recv(
    from: &mut FakeServerBox<HttpRequest, HttpResult>,
    expected: Option<HttpRequest>,
) {
    let actual = from.recv().await;
    tedge_http_ext::test_helpers::assert_request_eq(actual, expected)
}
