use crate::file_transfer_server::error::FileTransferError;
use anyhow::Context;
use axum::body::StreamBody;
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
use tokio::fs::File;
use tokio::io;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio_util::io::ReaderStream;

use super::error::FileTransferRequestError as Error;
use super::request_files::FileTransferPaths;

// TODO is this not just a repeat of code in tedge_config?
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
        Self {
            file_transfer_dir: data_dir.file_transfer_dir(),
            ..self
        }
    }

    pub fn with_port(self, port: u16) -> HttpConfig {
        let mut bind_address = self.bind_address;
        bind_address.set_port(port);
        Self {
            bind_address,
            ..self
        }
    }
}

fn separate_path_and_file_name(input: &Utf8Path) -> Option<(&Utf8Path, &str)> {
    Some((input.parent()?, input.file_name()?))
}

async fn upload_file(
    paths: FileTransferPaths,
    mut request: Request<Body>,
) -> Result<StatusCode, Error> {
    if let Some((directory, file_name)) = separate_path_and_file_name(&paths.full) {
        if let Err(err) = create_directories(directory) {
            return Err(Error::Upload {
                err: err.into(),
                path: paths.request,
            });
        }

        let full_path = directory.join(file_name);

        match stream_request_body_to_path(&full_path, request.body_mut()).await {
            Ok(()) => Ok(StatusCode::CREATED),
            Err(err) => Err(Error::Upload {
                err,
                path: paths.request,
            }),
        }
    } else {
        Err(Error::InvalidPath {
            path: paths.request,
        })
    }
}

async fn download_file(
    paths: FileTransferPaths,
) -> Result<StreamBody<ReaderStream<BufReader<File>>>, Error> {
    let reader: Result<_, io::Error> = async {
        let mut buf_reader = BufReader::new(File::open(paths.full).await?);
        // Filling the buffer will ensure the file can actually be read from,
        // which isn't true if it's a directory, but `File::open` alone won't
        // catch that
        buf_reader.fill_buf().await?;
        Ok(buf_reader)
    }
    .await;

    match reader {
        Ok(reader) => Ok(StreamBody::new(ReaderStream::new(reader))),
        Err(e) => {
            if e.kind() == ErrorKind::NotFound || err_is_is_a_directory(&e) {
                Err(Error::FileNotFound(paths.request))
            } else {
                Err(Error::FromIo(e))
            }
        }
    }
}

// Not a typo, snake_case for: 'err is "is a directory"'
fn err_is_is_a_directory(e: &std::io::Error) -> bool {
    // At the time of writing, `ErrorKind::IsADirectory` is feature-gated (https://github.com/rust-lang/rust/issues/86442)
    // Hence the string conversion rather than a direct comparison
    // If the error for reading a directory as a file changes, the unit tests should catch this
    e.kind().to_string() == "is a directory"
}

async fn delete_file(req: FileTransferPaths) -> Result<StatusCode, Error> {
    match tokio::fs::remove_file(&req.full).await {
        Ok(()) => Ok(StatusCode::ACCEPTED),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(StatusCode::ACCEPTED),
        Err(err) => Err(Error::DeleteIoError {
            err,
            path: req.request,
        }),
    }
}

async fn stream_request_body_to_path(
    path: &Utf8Path,
    body_stream: &mut hyper::Body,
) -> anyhow::Result<()> {
    let mut buffer = tokio::fs::File::create(path)
        .await
        .with_context(|| format!("creating {path:?}"))?;
    while let Some(data) = body_stream.next().await {
        let data =
            data.with_context(|| format!("reading body of uploaded file (destined for {path:?})"))?;
        let _bytes_written = buffer
            .write(&data)
            .await
            .with_context(|| format!("writing to {path:?}"))?;
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
    use axum::response::Response;
    use bytes::Bytes;
    use http_body::combinators::UnsyncBoxBody;
    use hyper::Method;
    use hyper::StatusCode;
    use tedge_test_utils::fs::TempTedgeDir;
    use test_case::test_case;
    use test_case::test_matrix;
    use tower::Service;
    use tower::ServiceExt;

    #[tokio::test]
    async fn file_is_uploaded_to_provided_data_dir() {
        let path = "some/dir/file";
        let (ttd, mut app) = app();
        let expected_output_file = ttd.utf8_path().join("file-transfer").join(path);

        upload_file(&mut app, path, "some content").await;

        assert_eq!(
            tokio::fs::read_to_string(expected_output_file)
                .await
                .unwrap(),
            "some content"
        );
    }

    #[tokio::test]
    async fn uploaded_file_can_be_downloaded_from_the_api() {
        let path = "some/dir/file";
        let (_ttd, mut app) = app();

        upload_file(&mut app, path, "some content").await;
        let response = download_file(&mut app, path).await;

        let body = hyper::body::to_bytes(response.into_body()).await.unwrap();

        assert_eq!(std::str::from_utf8(&body).unwrap(), "some content");
    }

    #[tokio::test]
    async fn uploaded_file_cannot_be_downloaded_after_deletion() {
        let path = "some/dir/file";
        let (_ttd, mut app) = app();

        upload_file(&mut app, path, "some content").await;
        delete_file(&mut app, path).await;
        let response = download_file(&mut app, path).await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
        assert_eq!(
            std::str::from_utf8(&body).unwrap(),
            "File not found: \"some/dir/file\""
        );
    }

    #[test]
    fn reading_a_directory_returns_a_directory_error() {
        // See comment in `err_is_is_a_directory` implementation
        // for why the function is tested in isolation
        let ttd = TempTedgeDir::new();
        ttd.dir("test-dir");

        let mut dir_path = ttd.path().to_owned();
        dir_path.push("test-dir");

        assert!(err_is_is_a_directory(
            &std::fs::read_to_string(&dir_path).unwrap_err()
        ))
    }

    #[test]
    fn reading_a_nonexistent_path_returns_a_non_directory_error() {
        // See comment in `err_is_is_a_directory` implementation
        // for why the function is tested in isolation
        let ttd = TempTedgeDir::new();

        let mut unknown_path = ttd.path().to_owned();
        unknown_path.push("not-a-real-file");

        assert!(!err_is_is_a_directory(
            &std::fs::read_to_string(&unknown_path).unwrap_err()
        ))
    }

    #[test_case("some/file" ; "with no trailing slash")]
    #[test_case("some/file/" ; "with trailing slash")]
    #[tokio::test]
    async fn upload_with_and_without_trailing_slash_has_same_effect(path: &str) {
        let (_ttd, mut app) = app();

        let response = upload_file(&mut app, path, "some content").await;

        assert_eq!(response.status(), StatusCode::CREATED);
    }

    #[test_matrix(
        ["/some/file", "../some/dir", "../../../some/other/dir"],
        [Method::GET, Method::PUT, Method::DELETE]
    )]
    #[tokio::test]
    async fn access_is_denied_if_request_attempts_path_traversal(path: &str, method: Method) {
        let (_ttd, mut app) = app();

        let req = Request::builder()
            .method(method)
            .uri(format!("/tedge/file-transfer/{path}"))
            .body(Body::empty())
            .expect("request builder");
        let response = app.call(req).await.unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
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

    async fn delete_file(
        app: &mut Router,
        path: &str,
    ) -> Response<UnsyncBoxBody<Bytes, axum::Error>> {
        let req = Request::builder()
            .method(Method::DELETE)
            .uri(format!("/tedge/file-transfer/{path}"))
            .body(Body::empty())
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
}
