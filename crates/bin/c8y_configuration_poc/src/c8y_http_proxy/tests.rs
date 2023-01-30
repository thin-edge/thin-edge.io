use crate::c8y_http_proxy::credentials::ConstJwtRetriever;
use crate::c8y_http_proxy::handle::C8YHttpProxy;
use crate::C8YHttpConfig;
use crate::C8YHttpProxyBuilder;
use c8y_api::json_c8y::InternalIdResponse;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::ServiceActor;
use tedge_actors::ServiceMessageBoxBuilder;
use tedge_http_ext::test_helpers::FakeHttpServerBox;
use tedge_http_ext::test_helpers::HttpResponseBuilder;
use tedge_http_ext::HttpRequestBuilder;

#[tokio::test]
async fn c8y_http_proxy_requests_the_device_internal_id_on_start() {
    let c8y_host = "c8y.tenant.io";
    let device_id = "device-001";
    let token = "some JWT token";
    let external_id = "external-device-001";

    let config = C8YHttpConfig {
        c8y_host: c8y_host.to_string(),
        device_id: device_id.to_string(),
    };
    let (mut proxy, mut c8y) = spawn_c8y_http_proxy(config, token).await;

    // Even before any request is sent to the c8y_proxy
    // the proxy requests over HTTP the internal device id.
    let init_request = HttpRequestBuilder::get(format!(
        "https://{c8y_host}/identity/externalIds/c8y_Serial/{device_id}"
    ))
    .bearer_auth(token)
    .build()
    .unwrap();
    c8y.assert_recv(Some(init_request)).await;

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
            .upload_log_binary("test.log", "some log content", None)
            .await
            .unwrap();
    });

    // then the upload request received by c8y is related to the internal id
    c8y.assert_recv(Some(
        HttpRequestBuilder::post(format!("https://{c8y_host}/event/events/"))
            .bearer_auth(token)
            .header("content-type", "application/json")
            .header("accept", "application/json")
            .build()
            .unwrap(),
    ))
    .await;
}

/// Spawn an `C8YHttpProxyActor` instance
/// Return two handles:
/// - one `C8YHttpProxy` to send requests to the actor
/// - one `ServiceMessageBoxBuilder<HttpRequest,HttpResponse> to fake the behavior of C8Y REST.
///
/// This also spawns an actor to generate fake JWT tokens.
/// The tests will only check that the http requests include this token.
async fn spawn_c8y_http_proxy(
    config: C8YHttpConfig,
    token: &str,
) -> (C8YHttpProxy, FakeHttpServerBox) {
    let jwt_actor = ServiceActor::new(ConstJwtRetriever {
        token: token.to_string(),
    });
    let mut jwt = ServiceMessageBoxBuilder::new("JWT Actor", 16);

    let mut http = FakeHttpServerBox::builder();

    let mut c8y_proxy_actor = C8YHttpProxyBuilder::new(config, &mut http, &mut jwt);
    let proxy = C8YHttpProxy::new("C8Y", &mut c8y_proxy_actor);

    tokio::spawn(jwt_actor.run(jwt.build()));
    tokio::spawn(c8y_proxy_actor.run());

    (proxy, http.build())
}
