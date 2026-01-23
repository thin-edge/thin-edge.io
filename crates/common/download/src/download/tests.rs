use super::*;
use axum::Router;
use hyper::header::AUTHORIZATION;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::PrivateKeyDer;
use rustls::RootCertStore;
use std::io::Write;
use std::sync::Arc;
use tempfile::tempdir;
use tempfile::NamedTempFile;
use tempfile::TempDir;
use test_case::test_case;

mod partial_response;

#[tokio::test]
async fn downloader_download_content_no_auth() {
    let mut server = mockito::Server::new_async().await;
    let _mock1 = server
        .mock("GET", "/some_file.txt")
        .with_status(200)
        .with_body(b"hello")
        .create_async()
        .await;

    let target_dir_path = TempDir::new().unwrap();
    let target_path = target_dir_path.path().join("test_download");

    let mut target_url = server.url();
    target_url.push_str("/some_file.txt");

    let url = DownloadInfo::new(&target_url);

    let mut downloader = Downloader::new(target_path, None, CloudHttpConfig::test_value());
    downloader.set_backoff(ExponentialBackoff {
        current_interval: Duration::ZERO,
        ..Default::default()
    });
    downloader.download(&url).await.unwrap();

    let log_content = std::fs::read(downloader.filename()).unwrap();

    assert_eq!("hello".as_bytes(), log_content);
}

#[tokio::test]
async fn downloader_download_to_target_path() {
    let temp_dir = tempdir().unwrap();

    let mut server = mockito::Server::new_async().await;
    let _mock1 = server
        .mock("GET", "/some_file.txt")
        .with_status(200)
        .with_body(b"hello")
        .create_async()
        .await;

    let target_path = temp_dir.path().join("downloaded_file.txt");

    let mut target_url = server.url();
    target_url.push_str("/some_file.txt");

    let url = DownloadInfo::new(&target_url);

    let downloader = Downloader::new(target_path.clone(), None, CloudHttpConfig::test_value());
    downloader.download(&url).await.unwrap();

    let file_content = std::fs::read(target_path).unwrap();

    assert_eq!(file_content, "hello".as_bytes());
}

#[cfg(target_os = "linux")]
#[tokio::test]
#[ignore = "Overriding Content-Length doesn't work in mockito"]
async fn downloader_download_with_content_length_larger_than_usable_disk_space() {
    use nix::sys::statvfs;
    let tmpstats = statvfs::statvfs("/tmp").unwrap();
    let usable_disk_space = (tmpstats.blocks_free() as u64) * (tmpstats.block_size() as u64);

    let mut server = mockito::Server::new_async().await;
    let _mock1 = server
        .mock("GET", "/some_file.txt")
        .with_header("content-length", &usable_disk_space.to_string())
        .create_async()
        .await;

    let target_dir_path = TempDir::new().unwrap();
    let target_path = target_dir_path.path().join("test_download_with_length");

    let mut target_url = server.url();
    target_url.push_str("/some_file.txt");

    let url = DownloadInfo::new(&target_url);

    let downloader = Downloader::new(target_path, None, CloudHttpConfig::test_value());
    let err = downloader.download(&url).await.unwrap_err();
    assert!(matches!(err, DownloadError::InsufficientSpace));
}

#[tokio::test]
async fn returns_proper_errors_for_invalid_filenames() {
    let temp_dir = tempdir().unwrap();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let mut server = mockito::Server::new_async().await;
    let _mock1 = server
        .mock("GET", "/some_file.txt")
        .with_status(200)
        .with_body(b"hello")
        .create_async()
        .await;

    let mut target_url = server.url();
    target_url.push_str("/some_file.txt");

    let url = DownloadInfo::new(&target_url);

    // empty filename
    let downloader = Downloader::new("".into(), None, CloudHttpConfig::test_value());
    let err = downloader.download(&url).await.unwrap_err();
    assert!(matches!(
        err,
        DownloadError::FromFileError(FileError::InvalidFileName { .. })
    ));

    // invalid unicode filename
    let path = unsafe { String::from_utf8_unchecked(b"\xff".to_vec()) };
    let downloader = Downloader::new(path.into(), None, CloudHttpConfig::test_value());
    let err = downloader.download(&url).await.unwrap_err();
    assert!(matches!(
        err,
        DownloadError::FromFileError(FileError::InvalidFileName { .. })
    ));

    // relative path filename
    let downloader = Downloader::new("myfile.txt".into(), None, CloudHttpConfig::test_value());
    let err = downloader.download(&url).await.unwrap_err();
    assert!(matches!(
        err,
        DownloadError::FromFileError(FileError::InvalidFileName { .. })
    ));
    println!("{err:?}", err = anyhow::Error::from(err));
}

#[tokio::test]
async fn writing_to_existing_file() {
    let temp_dir = tempdir().unwrap();
    let mut server = mockito::Server::new_async().await;
    let _mock1 = server
        .mock("GET", "/some_file.txt")
        .with_status(200)
        .with_body(b"hello")
        .create_async()
        .await;

    let target_file_path = temp_dir.path().join("downloaded_file.txt");
    std::fs::File::create(&target_file_path).unwrap();

    let mut target_url = server.url();
    target_url.push_str("/some_file.txt");

    let url = DownloadInfo::new(&target_url);

    let downloader = Downloader::new(
        target_file_path.clone(),
        None,
        CloudHttpConfig::test_value(),
    );
    downloader.download(&url).await.unwrap();

    let file_content = std::fs::read(target_file_path).unwrap();

    assert_eq!(file_content, "hello".as_bytes());
}

#[tokio::test]
async fn downloader_download_with_reasonable_content_length() {
    let file = create_file_with_size(10 * 1024 * 1024).unwrap();
    let file_path = file.into_temp_path();

    let mut server = mockito::Server::new_async().await;
    let _mock1 = server
        .mock("GET", "/some_file.txt")
        .with_body_from_file(&file_path)
        .create_async()
        .await;

    let target_dir_path = TempDir::new().unwrap();
    let target_path = target_dir_path.path().join("test_download_with_length");

    let mut target_url = server.url();
    target_url.push_str("/some_file.txt");

    let url = DownloadInfo::new(&target_url);

    let downloader = Downloader::new(target_path, None, CloudHttpConfig::test_value());

    downloader.download(&url).await.unwrap();

    let log_content = std::fs::read(downloader.filename()).unwrap();
    let expected_content = std::fs::read(file_path).unwrap();
    assert_eq!(log_content, expected_content);
}

#[tokio::test]
async fn downloader_download_verify_file_content() {
    let file = create_file_with_size(10).unwrap();

    let mut server = mockito::Server::new_async().await;
    let _mock1 = server
        .mock("GET", "/some_file.txt")
        .with_body_from_file(file.into_temp_path())
        .create_async()
        .await;

    let target_dir_path = TempDir::new().unwrap();
    let target_path = target_dir_path.path().join("test_download_with_length");

    let mut target_url = server.url();
    target_url.push_str("/some_file.txt");

    let url = DownloadInfo::new(&target_url);

    let downloader = Downloader::new(target_path, None, CloudHttpConfig::test_value());
    downloader.download(&url).await.unwrap();

    let log_content = std::fs::read(downloader.filename()).unwrap();

    assert_eq!("Some data!".as_bytes(), log_content);
}

#[tokio::test]
async fn downloader_download_without_content_length() {
    let mut server = mockito::Server::new_async().await;
    let _mock1 = server.mock("GET", "/some_file.txt").create_async().await;

    let target_dir_path = TempDir::new().unwrap();
    let target_path = target_dir_path.path().join("test_download_without_length");

    let mut target_url = server.url();
    target_url.push_str("/some_file.txt");

    let url = DownloadInfo::new(&target_url);

    let downloader = Downloader::new(target_path, None, CloudHttpConfig::test_value());
    downloader.download(&url).await.unwrap();

    assert_eq!("".as_bytes(), std::fs::read(downloader.filename()).unwrap());
}

#[tokio::test]
async fn doesnt_leave_tmpfiles_on_errors() {
    let server = mockito::Server::new_async().await;

    let target_dir_path = TempDir::new().unwrap();
    let target_path = target_dir_path.path().join("test_doesnt_leave_tmpfiles");

    let mut target_url = server.url();
    target_url.push_str("/some_file.txt");

    let url = DownloadInfo::new(&target_url);

    let mut downloader = Downloader::new(target_path, None, CloudHttpConfig::test_value());
    downloader.set_backoff(ExponentialBackoff {
        current_interval: Duration::ZERO,
        max_interval: Duration::ZERO,
        max_elapsed_time: Some(Duration::ZERO),
        ..Default::default()
    });
    downloader.download(&url).await.unwrap_err();

    assert_eq!(fs::read_dir(target_dir_path.path()).unwrap().count(), 0);
}

// Parameters:
//
// - status code
// - bearer token boolean
// - maybe url
// - expected std error
// - description
#[test_case(
        200,
        false,
        Some("not_a_url"),
        "URL"
        ; "builder error"
    )]
#[test_case(
        200,
        true,
        Some("not_a_url"),
        "URL"
        ; "builder error with auth"
    )]
#[test_case(
        200,
        false,
        Some("http://not_a_url"),
        "dns error"
        ; "dns error"
    )]
#[test_case(
        200,
        true,
        Some("http://not_a_url"),
        "dns error"
        ; "dns error with auth"
    )]
#[test_case(
        404,
        false,
        None,
        "404 Not Found"
        ; "client error"
    )]
#[test_case(
        404,
        true,
        None,
        "404 Not Found"
        ; "client error with auth"
    )]
#[tokio::test]
async fn downloader_download_processing_error(
    status_code: usize,
    with_token: bool,
    url: Option<&str>,
    expected_err: &str,
) {
    let target_dir_path = TempDir::new().unwrap();
    let mut server = mockito::Server::new_async().await;

    // bearer/no bearer setup
    let _mock1 = {
        if with_token {
            server
                .mock("GET", "/some_file.txt")
                .match_header("authorization", "Bearer token")
                .with_status(status_code)
                .create_async()
                .await
        } else {
            server
                .mock("GET", "/some_file.txt")
                .with_status(status_code)
                .create_async()
                .await
        }
    };

    // url/no url setup
    let url = {
        if let Some(url) = url {
            DownloadInfo::new(url)
        } else {
            let mut target_url = server.url();
            target_url.push_str("/some_file.txt");
            DownloadInfo::new(&target_url)
        }
    };

    // applying http auth header
    let url = {
        if with_token {
            let mut headers = HeaderMap::new();
            headers.append(AUTHORIZATION, "Bearer token".parse().unwrap());
            url.with_headers(headers)
        } else {
            url
        }
    };

    let target_path = target_dir_path.path().join("test_download");
    let mut downloader = Downloader::new(target_path, None, CloudHttpConfig::test_value());
    downloader.set_backoff(ExponentialBackoff {
        max_elapsed_time: Some(Duration::ZERO),
        ..Default::default()
    });
    match downloader.download(&url).await {
        Ok(_success) => panic!("Expected client error."),
        Err(err) => {
            // `Error::to_string` uses a Display trait and only contains a
            // top-level error message, and not any lower level contexts. To
            // make sure that we look at the entire error chain, we wrap the
            // error in `anyhow::Error` which reports errors by printing the
            // entire error chain. We can then check keywords that we want
            // appear somewhere in the error chain

            let err = anyhow::Error::from(err);
            println!("{err:?}");

            // We use debug representation because that's what anyhow uses
            // to pretty print error report chain
            assert!(format!("{err:?}")
                .to_ascii_lowercase()
                .contains(&expected_err.to_ascii_lowercase()));
        }
    };
}

#[tokio::test]
async fn downloader_error_shows_certificate_required_error_when_appropriate() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server_cert = rcgen::generate_simple_self_signed(["localhost".into()]).unwrap();
    let cert = server_cert.cert.der().clone();
    let key =
        PrivateKeyDer::from_pem_slice(server_cert.signing_key.serialize_pem().as_bytes()).unwrap();
    let mut accepted_certs = RootCertStore::empty();
    accepted_certs.add(cert.clone()).unwrap();
    let config = axum_tls::ssl_config(vec![cert.clone()], key, Some(accepted_certs)).unwrap();
    let app = Router::new();

    tokio::spawn(axum_tls::start_tls_server(
        listener.into_std().unwrap(),
        config,
        app,
    ));

    let req_cert = reqwest::Certificate::from_der(&cert).unwrap();
    let url = DownloadInfo::new(&format!("http://localhost:{port}"));

    let downloader = Downloader::new(
        PathBuf::from("/tmp/should-never-exist"),
        None,
        CloudHttpConfig::new(Arc::from(vec![req_cert]), None),
    );
    let err = downloader.download(&url).await.unwrap_err();
    let err = anyhow::Error::new(err);

    assert!(dbg!(format!("{err:#}")).contains("received fatal alert: CertificateRequired"));
}

fn create_file_with_size(size: usize) -> Result<NamedTempFile, anyhow::Error> {
    let mut file = NamedTempFile::new().unwrap();
    let data: String = "Some data!".into();
    let loops = size / data.len();
    let mut buffer = String::with_capacity(size);
    for _ in 0..loops {
        buffer.push_str("Some data!");
    }

    file.write_all(buffer.as_bytes()).unwrap();
    file.flush().unwrap();

    Ok(file)
}
