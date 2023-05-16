use super::*;
use download::Auth;
use mockito::mock;
use std::time::Duration;
use tedge_actors::ClientMessageBox;
use tedge_actors::DynError;
use tedge_test_utils::fs::TempTedgeDir;
use tokio::time::timeout;

const TEST_TIMEOUT: Duration = Duration::from_secs(5);

#[tokio::test]
async fn download_without_auth() -> Result<(), DynError> {
    let ttd = TempTedgeDir::new();
    let _mock = mock("GET", "/")
        .with_status(200)
        .with_header("content-type", "text/plain")
        .with_body("without auth")
        .create();

    let target_path = ttd.path().join("downloaded_file");
    let server_url = mockito::server_url();
    let download_request = DownloadRequest::new(&server_url, &target_path, None);

    let mut requester = spawn_downloader_actor().await;

    let (id, response) = timeout(
        TEST_TIMEOUT,
        requester.await_response(("id".to_string(), download_request)),
    )
    .await?
    .expect("timeout");

    assert_eq!(id.as_str(), "id");
    assert!(response.is_ok());
    assert_eq!(response.as_ref().unwrap().file_path, target_path.as_path());
    assert_eq!(response.as_ref().unwrap().url, server_url);

    Ok(())
}

#[tokio::test]
async fn download_with_auth() -> Result<(), DynError> {
    let ttd = TempTedgeDir::new();
    let _mock = mock("GET", "/")
        .with_status(200)
        .with_header("content-type", "text/plain")
        .match_header("authorization", "Bearer token")
        .with_body("with auth")
        .create();

    let target_path = ttd.path().join("downloaded_file");
    let server_url = mockito::server_url();
    let download_request = DownloadRequest::new(
        &server_url,
        &target_path,
        Some(Auth::Bearer("token".into())),
    );

    let mut requester = spawn_downloader_actor().await;

    let (id, response) = timeout(
        TEST_TIMEOUT,
        requester.await_response(("id".to_string(), download_request)),
    )
    .await?
    .expect("timeout");

    assert_eq!(id.as_str(), "id");
    assert!(response.is_ok());
    assert_eq!(response.as_ref().unwrap().file_path, target_path.as_path());
    assert_eq!(response.as_ref().unwrap().url, server_url);

    Ok(())
}

async fn spawn_downloader_actor(
) -> ClientMessageBox<(String, DownloadRequest), (String, DownloadResult)> {
    let mut downloader_actor_builder = DownloaderActor::new().builder();
    let requester = ClientMessageBox::new("DownloadRequester", &mut downloader_actor_builder);

    tokio::spawn(downloader_actor_builder.run());

    requester
}
