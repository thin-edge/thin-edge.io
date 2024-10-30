use crate::*;
use tedge_actors::ClientMessageBox;
use tedge_config::TEdgeConfigLocation;
use tedge_test_utils::fs::TempTedgeDir;

#[tokio::test]
async fn get_over_https() {
    let mut server = mockito::Server::new_async().await;
    let _mock = server.mock("GET", "/").create_async().await;

    let mut http = spawn_http_actor().await;

    let request = HttpRequestBuilder::get(server.url())
        .build()
        .expect("A simple HTTPS GET request");

    let response = http.await_response(request).await.expect("some response");
    assert!(response.is_ok());
    assert_eq!(response.unwrap().status(), 200);
}

async fn spawn_http_actor() -> ClientMessageBox<HttpRequest, HttpResult> {
    let ttd = TempTedgeDir::new();
    let config_loc = TEdgeConfigLocation::from_custom_root(ttd.path());
    let config = config_loc.load().unwrap();
    let mut builder = HttpActor::new(&config).builder();
    let handle = ClientMessageBox::new(&mut builder);

    tokio::spawn(builder.run());

    handle
}
