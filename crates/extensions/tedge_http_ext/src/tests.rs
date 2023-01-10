use crate::*;

#[tokio::test]
async fn get_over_https() {
    let mut http = spawn_http_actor(HttpConfig::default()).await;

    let request = HttpRequestBuilder::get("https://httpbin.org/get")
        .build()
        .expect("A simple HTTPS GET request");

    let response = http.await_response(request).await.expect("some response");
    assert!(response.is_ok());
    assert_eq!(response.unwrap().status(), 200);
}

async fn spawn_http_actor(config: HttpConfig) -> RequestResponseHandler<HttpRequest, HttpResult> {
    let mut builder = HttpActorBuilder::new(config).unwrap();
    let handle = builder.box_builder.add_client("Tester");

    tokio::spawn(builder.run());

    handle
}
