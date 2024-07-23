use std::fs::File;
use std::io::Write;
use std::time::Duration;

use camino::Utf8Path;
use tedge_actors::ClientMessageBox;
use tedge_actors::DynError;
use tedge_test_utils::fs::TempTedgeDir;
use certificate::CloudRootCerts;
use tokio::time::timeout;
use upload::Auth;

use crate::UploadRequest;
use crate::UploadResult;
use crate::UploaderActor;

const TEST_TIMEOUT: Duration = Duration::from_secs(5);

async fn spawn_uploader_actor() -> ClientMessageBox<(String, UploadRequest), (String, UploadResult)>
{
    let identity = None;
    let mut uploader_actor_builder =
        UploaderActor::new(identity, CloudRootCerts::from([])).builder();
    let requester = ClientMessageBox::new(&mut uploader_actor_builder);

    tokio::spawn(uploader_actor_builder.run());

    requester
}

#[tokio::test]
async fn upload_without_auth() -> Result<(), DynError> {
    let ttd = TempTedgeDir::new();
    let mut server = mockito::Server::new();
    let _mock = server.mock("PUT", "/").with_status(201).create();

    let target_path = Utf8Path::from_path(ttd.path())
        .unwrap()
        .join("file_to_upload.txt");

    let mut tmp_file = File::create(&target_path).unwrap();
    tmp_file.write_all(b"Hello, world!").unwrap();

    let server_url = server.url();
    let download_request = UploadRequest::new(&server_url, &target_path);

    let mut requester = spawn_uploader_actor().await;

    let (id, response) = timeout(
        TEST_TIMEOUT,
        requester.await_response(("id".to_string(), download_request)),
    )
    .await
    .expect("timeout")?;

    assert_eq!(id.as_str(), "id");
    assert!(response.is_ok());
    assert_eq!(response.as_ref().unwrap().file_path, target_path.as_path());
    assert_eq!(response.as_ref().unwrap().url, server_url);

    Ok(())
}

#[tokio::test]
async fn upload_with_auth() -> Result<(), DynError> {
    let ttd = TempTedgeDir::new();
    let mut server = mockito::Server::new();
    let _mock = server
        .mock("PUT", "/")
        .match_header("authorization", "Bearer token")
        .with_status(201)
        .create();

    let target_path = Utf8Path::from_path(ttd.path())
        .unwrap()
        .join("file_to_upload.txt");

    let mut tmp_file = File::create(&target_path).unwrap();
    tmp_file.write_all(b"Hello, world!").unwrap();

    let server_url = server.url();
    let download_request =
        UploadRequest::new(&server_url, &target_path).with_auth(Auth::Bearer("token".into()));

    let mut requester = spawn_uploader_actor().await;

    let (id, response) = timeout(
        TEST_TIMEOUT,
        requester.await_response(("id".to_string(), download_request)),
    )
    .await
    .expect("timeout")?;

    assert_eq!(id.as_str(), "id");
    assert!(response.is_ok());
    assert_eq!(response.as_ref().unwrap().file_path, target_path.as_path());
    assert_eq!(response.as_ref().unwrap().url, server_url);

    Ok(())
}
