use super::*;
use download::Auth;
use std::time::Duration;
use tedge_actors::ClientMessageBox;
use tedge_test_utils::fs::TempTedgeDir;
use tedge_utils::certificates::CloudRootCerts;
use tedge_utils::file::PermissionEntry;
use tokio::time::timeout;

const TEST_TIMEOUT: Duration = Duration::from_secs(5);

#[tokio::test]
async fn download_without_auth() {
    let ttd = TempTedgeDir::new();
    let mut server = mockito::Server::new();
    let _mock = server
        .mock("GET", "/")
        .with_status(200)
        .with_header("content-type", "text/plain")
        .with_body("without auth")
        .create();

    let target_path = ttd.path().join("downloaded_file");
    let server_url = server.url();
    let download_request = DownloadRequest::new(&server_url, &target_path);

    let mut requester = spawn_downloader_actor().await;

    let (id, response) = timeout(
        TEST_TIMEOUT,
        requester.await_response(("id".to_string(), download_request)),
    )
    .await
    .expect("timeout")
    .expect("channel error");

    assert_eq!(id.as_str(), "id");
    assert_eq!(response.as_ref().unwrap().file_path, target_path.as_path());
    assert_eq!(response.as_ref().unwrap().url, server_url);
}

#[tokio::test]
async fn download_with_auth() {
    let ttd = TempTedgeDir::new();
    let mut server = mockito::Server::new();
    let _mock = server
        .mock("GET", "/")
        .with_status(200)
        .with_header("content-type", "text/plain")
        .match_header("authorization", "Bearer token")
        .with_body("with auth")
        .create();

    let target_path = ttd.path().join("downloaded_file");
    let server_url = server.url();
    let download_request =
        DownloadRequest::new(&server_url, &target_path).with_auth(Auth::Bearer("token".into()));

    let mut requester = spawn_downloader_actor().await;

    let (id, response) = timeout(
        TEST_TIMEOUT,
        requester.await_response(("id".to_string(), download_request)),
    )
    .await
    .expect("timeout")
    .expect("channel error");

    assert_eq!(id.as_str(), "id");
    assert_eq!(response.as_ref().unwrap().file_path, target_path.as_path());
    assert_eq!(response.as_ref().unwrap().url, server_url);
}

#[tokio::test]
async fn download_with_permission() {
    let ttd = TempTedgeDir::new();
    let mut server = mockito::Server::new();
    let _mock = server
        .mock("GET", "/")
        .with_status(200)
        .with_header("content-type", "text/plain")
        .with_body("without auth")
        .create();

    let target_path = ttd.path().join("downloaded_file");
    let server_url = server.url();
    let user = whoami::username();
    let group = uzers::get_current_groupname()
        .unwrap()
        .into_string()
        .unwrap();

    let download_request = DownloadRequest::new(&server_url, &target_path)
        .with_permission(PermissionEntry::new(Some(user), Some(group), Some(0o775)));

    let mut requester = spawn_downloader_actor().await;

    let (id, response) = timeout(
        TEST_TIMEOUT,
        requester.await_response(("id".to_string(), download_request)),
    )
    .await
    .expect("timeout")
    .expect("channel error");

    assert_eq!(id.as_str(), "id");
    assert_eq!(response.as_ref().unwrap().file_path, target_path.as_path());
    assert_eq!(response.as_ref().unwrap().url, server_url);
}

async fn spawn_downloader_actor(
) -> ClientMessageBox<(String, DownloadRequest), (String, DownloadResult)> {
    let mut downloader_actor_builder =
        DownloaderActor::new(None, CloudRootCerts::from([])).builder();
    let requester = ClientMessageBox::new(&mut downloader_actor_builder);

    tokio::spawn(downloader_actor_builder.run());

    requester
}

#[tokio::test]
async fn download_if_download_key_is_struct() {
    let ttd = TempTedgeDir::new();
    let mut server = mockito::Server::new();
    let _mock = server
        .mock("GET", "/")
        .with_status(200)
        .with_header("content-type", "text/plain")
        .with_body("without auth")
        .create();

    let target_path = ttd.path().join("downloaded_file");
    let server_url = server.url();
    let download_request = DownloadRequest::new(&server_url, &target_path);
    let request_key = TestDownloadKey {
        text: "I am test".to_string(),
        some: true,
    };

    let mut requester = spawn_downloader_actor_with_struct().await;

    let (return_key, response) = timeout(
        TEST_TIMEOUT,
        requester.await_response((request_key.clone(), download_request)),
    )
    .await
    .expect("timeout")
    .expect("channel error");

    assert_eq!(return_key, request_key);
    assert_eq!(response.as_ref().unwrap().file_path, target_path.as_path());
    assert_eq!(response.as_ref().unwrap().url, server_url);
}

#[derive(Default, Debug, PartialEq, Eq, Clone)]
struct TestDownloadKey {
    text: String,
    some: bool,
}

async fn spawn_downloader_actor_with_struct(
) -> ClientMessageBox<(TestDownloadKey, DownloadRequest), (TestDownloadKey, DownloadResult)> {
    let mut downloader_actor_builder =
        DownloaderActor::new(None, CloudRootCerts::from([])).builder();
    let requester = ClientMessageBox::new(&mut downloader_actor_builder);

    tokio::spawn(downloader_actor_builder.run());

    requester
}
