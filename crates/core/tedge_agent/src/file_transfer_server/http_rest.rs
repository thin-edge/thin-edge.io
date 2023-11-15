use crate::file_transfer_server::error::FileTransferError;
use axum::extract::Path;
use axum::extract::State;
use axum::routing::get;
use axum::routing::IntoMakeService;
use axum::Router;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use hyper::server::conn::AddrIncoming;
use hyper::Body;
use hyper::Request;
use hyper::Server;
use hyper::StatusCode;
use std::io::ErrorKind;
use std::net::IpAddr;
use std::net::SocketAddr;
use std::sync::Arc;
use tedge_actors::futures::StreamExt;
use tedge_api::path::DataDir;
use tedge_utils::paths::create_directories;
use tokio::io::AsyncWriteExt;

use super::error::FileTransferRequestError;

const HTTP_FILE_TRANSFER_PORT: u16 = 8000;

#[derive(Debug, Clone)]
pub struct HttpConfig {
    pub bind_address: SocketAddr,
    pub file_transfer_dir: Utf8PathBuf,
}

impl Default for HttpConfig {
    fn default() -> Self {
        HttpConfig {
            bind_address: ([127, 0, 0, 1], HTTP_FILE_TRANSFER_PORT).into(),
            file_transfer_dir: DataDir::default().file_transfer_dir(),
        }
    }
}

impl HttpConfig {
    pub fn with_ip_address(self, ip_address: IpAddr) -> HttpConfig {
        Self {
            bind_address: SocketAddr::new(ip_address, self.bind_address.port()),
            ..self
        }
    }

    pub fn with_data_dir(self, data_dir: DataDir) -> HttpConfig {
        Self { file_transfer_dir: data_dir.file_transfer_dir(), ..self }
    }

    pub fn with_port(self, port: u16) -> HttpConfig {
        let mut bind_address = self.bind_address;
        bind_address.set_port(port);
        Self {
            bind_address,
            ..self
        }
    }

    /// Return the path of the file associated to the given `uri`
    ///
    /// Check that:
    /// * the `uri` is related to the file-transfer i.e a sub-uri of `self.file_transfer_uri`
    /// * the `path`, once normalized, is actually under `self.file_transfer_dir`
    pub fn local_path_for_file(
        &self,
        path: &Utf8Path,
    ) -> Result<Utf8PathBuf, FileTransferRequestError> {
        let full_path = self.file_transfer_dir.join(path);

        // TODO rocket does this in a nicer way (I think), that I'd like to copy
        // One must check that once normalized (i.e. any `..` removed)
        // the path is still under the file transfer dir
        let clean_path = clean_utf8_path(&full_path);
        // TODO did we actually mean to allow (very limited) path traversal attacks into the data dir?
        if clean_path.starts_with(&self.file_transfer_dir) {
            Ok(clean_path)
        } else {
            Err(FileTransferRequestError::InvalidURI {
                value: clean_path.to_string(),
            })
        }
    }
}

fn clean_utf8_path(path: &Utf8Path) -> Utf8PathBuf {
    Utf8PathBuf::from(path_clean::clean(path.as_str()))
}

// TODO I think this methodology is flawed - it has unit tests, but the cases we test can't be reached due to clean_utf8_path interfering 
fn separate_path_and_file_name(input: &Utf8Path) -> Option<(&Utf8Path, &str)> {
    let (relative_path, file_name) = input.as_str().rsplit_once('/')?;

    let relative_path = Utf8Path::new(relative_path);
    Some((relative_path, file_name))
}

async fn upload_file(
    Path(path): Path<Utf8PathBuf>,
    file_transfer: State<Arc<HttpConfig>>,
    mut request: Request<Body>,
) -> Result<StatusCode, FileTransferRequestError> {
    let full_path = file_transfer.local_path_for_file(&path)?;

    if let Some((relative_path, file_name)) = separate_path_and_file_name(&full_path) {
        let root_path = file_transfer.file_transfer_dir.clone();
        let directories_path = root_path.join(relative_path);

        if let Err(_err) = create_directories(&directories_path) {
            return Ok(StatusCode::FORBIDDEN);
        }

        let full_path = directories_path.join(file_name);

        match stream_request_body_to_path(&full_path, request.body_mut()).await {
            Ok(()) => Ok(StatusCode::CREATED),
            Err(_err) => {
                // TODO add error variant
                Ok(StatusCode::FORBIDDEN)
            }
        }
    } else {
        // TODO add error variant
        Ok(StatusCode::FORBIDDEN)
    }
}

async fn download_file(
    Path(path): Path<Utf8PathBuf>,
    file_transfer: State<Arc<HttpConfig>>,
) -> Result<Vec<u8>, FileTransferRequestError> {
    let full_path = file_transfer.local_path_for_file(&path)?;

    // TODO do we really want to read this entirely into memory?
    match tokio::fs::read(full_path).await {
        Ok(contents) => Ok(contents),
        Err(e) => {
            if e.kind() == ErrorKind::NotFound || err_is_is_a_directory(&e) {
                Err(FileTransferRequestError::FileNotFound(path))
            } else {
                Err(FileTransferRequestError::FromIo(e))
            }
        }
    }
}

fn err_is_is_a_directory(e: &std::io::Error) -> bool {
    // At the time of writing, `ErrorKind::IsADirectory` is feature-gated (https://github.com/rust-lang/rust/issues/86442)
    // Hence the string conversion rather than a direct comparison
    // If the error for reading a directory as a file changes, the unit tests should catch this
    e.kind().to_string() == "is a directory"
}

async fn delete_file(
    Path(path): Path<Utf8PathBuf>,
    file_transfer: State<Arc<HttpConfig>>,
) -> Result<StatusCode, FileTransferRequestError> {
    let full_path = file_transfer.local_path_for_file(&path)?;

    match tokio::fs::remove_file(&full_path).await {
        Ok(()) => Ok(StatusCode::ACCEPTED),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(StatusCode::ACCEPTED),
        Err(err) => Err(FileTransferRequestError::DeleteIoError { err, path }),
    }
}

async fn stream_request_body_to_path(
    path: &Utf8Path,
    body_stream: &mut hyper::Body,
) -> Result<(), FileTransferError> {
    let mut buffer = tokio::fs::File::create(path).await?;
    while let Some(data) = body_stream.next().await {
        let data = data?;
        let _bytes_written = buffer.write(&data).await?;
    }
    Ok(())
}

pub fn http_file_transfer_server(
    config: HttpConfig,
) -> Result<Server<AddrIncoming, IntoMakeService<Router>>, FileTransferError> {
    let bind_address = config.bind_address;
    let router = http_file_transfer_router(config);
    let server_builder = Server::try_bind(&bind_address);
    match server_builder {
        Ok(server) => Ok(server.serve(router.into_make_service())),
        Err(_err) => Err(FileTransferError::BindingAddressInUse {
            address: bind_address,
        }),
    }
}

fn http_file_transfer_router(config: HttpConfig) -> Router {
    Router::new()
        .route(
            "/tedge/file-transfer/*path",
            get(download_file).put(upload_file).delete(delete_file),
        )
        .with_state(Arc::new(config))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use http_body::combinators::UnsyncBoxBody;
    use hyper::Method;
    use axum::response::Response;
    use hyper::StatusCode;
    use tedge_test_utils::fs::TempTedgeDir;
    use test_case::test_case;
    use tower::Service;
    use tower::ServiceExt;

    #[test_case(
        "some/dir/file",
        Some(Utf8PathBuf::from("/var/tedge/file-transfer/some/dir/file"))
    )]
    fn test_remove_prefix_from_uri(input: &str, output: Option<Utf8PathBuf>) {
        let file_transfer = HttpConfig::default();
        let actual_output = file_transfer.local_path_for_file(Utf8Path::new(input)).ok();
        assert_eq!(actual_output, output);
    }

    #[test]
    fn reading_a_directory_returns_directory_error() {
        let ttd = TempTedgeDir::new();
        ttd.dir("test-dir");

        let mut dir_path = ttd.path().to_owned();
        dir_path.push("test-dir");

        assert!(err_is_is_a_directory(
            &std::fs::read_to_string(&dir_path).unwrap_err()
        ))
    }

    #[test]
    fn not_found_returns_a_non_directory_error() {
        let ttd = TempTedgeDir::new();

        let mut unknown_path = ttd.path().to_owned();
        unknown_path.push("not-a-real-file");

        assert!(!err_is_is_a_directory(
            &std::fs::read_to_string(&unknown_path).unwrap_err()
        ))
    }

    #[test_case("/tedge/some/dir/file", "/tedge/some/dir", "file")]
    #[test_case("/tedge/some/dir/", "/tedge/some/dir", "")]
    fn test_separate_path_and_file_name(
        input: &str,
        expected_path: &str,
        expected_file_name: &str,
    ) {
        let (actual_path, actual_file_name) =
            separate_path_and_file_name(Utf8Path::new(input)).unwrap();
        assert_eq!(actual_path, expected_path);
        assert_eq!(actual_file_name, expected_file_name);
    }

    const VALID_TEST_URI: &str = "/tedge/file-transfer/another/dir/test-file";
    const INVALID_TEST_URI: &str = "/wrong/place/test-file";

    #[test_case(Method::GET, VALID_TEST_URI, StatusCode::OK)]
    #[test_case(Method::GET, INVALID_TEST_URI, StatusCode::NOT_FOUND)]
    #[test_case(Method::DELETE, VALID_TEST_URI, StatusCode::ACCEPTED)]
    #[test_case(Method::DELETE, INVALID_TEST_URI, StatusCode::NOT_FOUND)]
    #[tokio::test]
    async fn test_file_transfer_http_methods(
        method: hyper::Method,
        uri: &'static str,
        status_code: hyper::StatusCode,
    ) {
        let (_ttd, mut app) = app();
        let req = Request::builder()
            .method(method)
            .uri(uri)
            .body(Body::empty())
            .expect("request builder");

        app.call(client_put_request()).await.unwrap();
        let response = app.call(req).await.unwrap();

        assert_eq!(response.status(), status_code);
    }

    #[test_case(VALID_TEST_URI, hyper::StatusCode::CREATED)]
    #[test_case(INVALID_TEST_URI, hyper::StatusCode::NOT_FOUND)]
    #[tokio::test]
    async fn test_file_transfer_put(uri: &'static str, status_code: hyper::StatusCode) {
        let body = "just an example body";
        let req = Request::builder()
            .method(Method::PUT)
            .uri(uri)
            .body(Body::from(body))
            .expect("request builder");

        let (_ttd, app) = app();

        let response = app.oneshot(req).await.unwrap();

        assert_eq!(response.status(), status_code);
    }

    #[tokio::test]
    async fn get_responds_with_not_found_for_nonexistent_file() {
        let req = Request::builder()
            .method(Method::GET)
            .uri(VALID_TEST_URI)
            .body(Body::empty())
            .expect("request builder");

        let (_ttd, app) = app();

        let response = app.oneshot(req).await.unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn get_responds_with_not_found_for_directory() {
        let (_ttd, mut app) = app();

        upload_file(&mut app, "dir/a-file.txt", "some content").await;

        let response = download_file(&mut app, "dir").await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    async fn upload_file(
        app: &mut Router,
        path: &str,
        contents: &str,
    ) -> Response<UnsyncBoxBody<Bytes, axum::Error>> {
        let req = Request::builder()
            .method(Method::PUT)
            .uri(format!("/tedge/file-transfer/{path}"))
            .body(Body::from(contents.to_owned()))
            .expect("request builder");

        app.call(req).await.unwrap()
    }

    async fn download_file(
        app: &mut Router,
        path: &str,
    ) -> Response<UnsyncBoxBody<Bytes, axum::Error>> {
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/tedge/file-transfer/{path}"))
            .body(Body::empty())
            .expect("request builder");

        app.call(req).await.unwrap()
    }

    fn app() -> (TempTedgeDir, Router) {
        let ttd = TempTedgeDir::new();
        let tempdir_path = ttd.utf8_path_buf();
        let http_config = HttpConfig::default()
            .with_data_dir(tempdir_path.into())
            .with_port(3333);
        let router = http_file_transfer_router(http_config);
        (ttd, router)
    }

    // canonicalised client PUT request to create a file in `VALID_TEST_URI`
    // this is to be used to test the GET and DELETE methods.
    fn client_put_request() -> Request<Body> {
        Request::builder()
            .method(Method::PUT)
            .uri(VALID_TEST_URI)
            .body(Body::from("file transfer server"))
            .expect("request builder")
    }

    #[test_case(String::from("../../../bin/sh"), false)]
    #[test_case(
        String::from("../file-transfer/new/dir/file"),
        true
    )]
    fn test_verify_uri(uri: String, is_ok: bool) {
        let file_transfer = HttpConfig::default();
        let res = file_transfer.local_path_for_file(Utf8Path::new(&uri));
        match is_ok {
            true => {
                assert!(res.is_ok());
            }
            false => {
                assert!(res.is_err());
            }
        }
    }
}
