use crate::*;
use tedge_actors::ClientMessageBox;

#[tokio::test]
async fn get_over_https() {
    let mut http = spawn_http_actor().await;

    let request = HttpRequestBuilder::get("https://httpbin.org/get")
        .build()
        .expect("A simple HTTPS GET request");

    let response = http.await_response(request).await.expect("some response");
    assert!(response.is_ok());
    assert_eq!(response.unwrap().status(), 200);
}

async fn spawn_http_actor() -> ClientMessageBox<HttpRequest, HttpResult> {
    let mut builder = HttpActorBuilder::new().unwrap();
    let handle = ClientMessageBox::new("Tester", &mut builder.box_builder, NoConfig);

    tokio::spawn(builder.run());

    handle
}
