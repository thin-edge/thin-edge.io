use crate::file_transfer_server::error::FileTransferError;
use anyhow::anyhow;
use anyhow::Context;
use axum::body::StreamBody;
use axum::routing::get;
use axum::Router;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use futures::future::FutureExt;
use hyper::Body;
use hyper::Request;
use hyper::StatusCode;
use rustls::ServerConfig;
use std::future::Future;
use std::io::ErrorKind;
use tedge_actors::futures::StreamExt;
use tedge_utils::paths::create_directories;
use tokio::fs::File;
use tokio::io;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::net::TcpListener;
use tokio_util::io::ReaderStream;

use super::error::FileTransferRequestError as Error;
use super::request_files::FileTransferDir;
use super::request_files::FileTransferPath;
use super::request_files::RequestPath;

async fn upload_file(
    path: FileTransferPath,
    mut request: Request<Body>,
) -> Result<StatusCode, Error> {
    fn internal_error(source: impl Into<anyhow::Error>, path: RequestPath) -> Error {
        Error::Upload {
            source: source.into(),
            path,
        }
    }

    if let Some(directory) = path.full.parent() {
        if let Err(err) = create_directories(directory) {
            return Err(internal_error(err, path.request));
        }

        match stream_request_body_to_path(&path.full, request.body_mut()).await {
            Ok(()) => Ok(StatusCode::CREATED),
            Err(err) if source_err_is_is_a_directory(&err, &path.full) => {
                Err(Error::CannotUploadDirectory { path: path.request })
            }
            Err(err) => Err(internal_error(err, path.request)),
        }
    } else {
        Err(internal_error(
            anyhow!("cannot retrieve directory name for {}", path.full),
            path.request,
        ))
    }
}

fn source_err_is_is_a_directory(error: &anyhow::Error, path: &Utf8Path) -> bool {
    error
        .downcast_ref()
        .map(|e| err_is_is_a_directory(e, path))
        .unwrap_or(false)
}

async fn download_file(
    path: FileTransferPath,
) -> Result<StreamBody<ReaderStream<BufReader<File>>>, Error> {
    let reader: Result<_, io::Error> = async {
        let mut buf_reader = BufReader::new(File::open(&path.full).await?);
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
            if e.kind() == ErrorKind::NotFound || err_is_is_a_directory(&e, &path.full) {
                Err(Error::FileNotFound(path.request))
            } else {
                Err(Error::FromIo(e))
            }
        }
    }
}

// Not a typo, snake_case for: 'err is "is a directory"'
fn err_is_is_a_directory(e: &io::Error, path: &Utf8Path) -> bool {
    // At the time of writing, `ErrorKind::IsADirectory` is feature-gated (https://github.com/rust-lang/rust/issues/86442)
    // Hence the string conversion rather than a direct comparison
    // If the error for reading a directory as a file changes, the unit tests should catch this
    // On some OS's like MacOS, an ambiguous "permission denied" error is returned
    // when trying to delete a file which is actually directory.
    e.kind().to_string() == "is a directory" || path.is_dir()
}

async fn delete_file(path: FileTransferPath) -> Result<StatusCode, Error> {
    match tokio::fs::remove_file(&path.full).await {
        Ok(()) => Ok(StatusCode::ACCEPTED),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(StatusCode::ACCEPTED),
        Err(e) if err_is_is_a_directory(&e, &path.full) => {
            Err(Error::CannotDeleteDirectory { path: path.request })
        }
        Err(err) => Err(Error::Delete {
            source: err,
            path: path.request,
        }),
    }
}

async fn stream_request_body_to_path(
    path: &Utf8Path,
    body_stream: &mut Body,
) -> anyhow::Result<()> {
    let mut buffer = File::create(path)
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

pub(crate) fn http_file_transfer_server(
    listener: TcpListener,
    file_transfer_dir: Utf8PathBuf,
    rustls_config: Option<ServerConfig>,
) -> Result<impl Future<Output = io::Result<()>>, FileTransferError> {
    let router = http_file_transfer_router(file_transfer_dir);
    let listener = listener.into_std()?;

    let server = if let Some(rustls_config) = rustls_config {
        axum_tls::start_tls_server(listener, rustls_config, router).boxed()
    } else {
        axum_server::from_tcp(listener)
            .serve(router.into_make_service())
            .boxed()
    };

    Ok(server)
}

fn http_file_transfer_router(file_transfer_dir: Utf8PathBuf) -> Router {
    Router::new()
        .route(
            "/tedge/file-transfer/*path",
            get(download_file).put(upload_file).delete(delete_file),
        )
        .with_state(FileTransferDir::new(file_transfer_dir))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::response::Response;
    use bytes::Bytes;
    use http_body::combinators::UnsyncBoxBody;
    use hyper::Method;
    use hyper::StatusCode;
    use tedge_api::path::DataDir;
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
            &std::fs::read_to_string(&dir_path).unwrap_err(),
            Utf8Path::from_path(dir_path.as_path()).unwrap()
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
            &std::fs::read_to_string(&unknown_path).unwrap_err(),
            Utf8Path::from_path(unknown_path.as_path()).unwrap()
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

    #[test_case(Method::GET, StatusCode::NOT_FOUND)]
    #[test_case(Method::PUT, StatusCode::CONFLICT)]
    #[test_case(Method::DELETE, StatusCode::NOT_FOUND)]
    #[tokio::test]
    async fn all_methods_respond_with_sensible_status_for_a_directory(
        method: Method,
        status_code: StatusCode,
    ) {
        let (_ttd, mut app) = app();

        upload_file(&mut app, "dir/a-file.txt", "some content").await;

        let response = request_with(method, &mut app, "dir", "").await;

        assert_eq!(response.status(), status_code);
    }

    async fn request_with(
        method: Method,
        app: &mut Router,
        path: &str,
        body: impl Into<Body>,
    ) -> Response<UnsyncBoxBody<Bytes, axum::Error>> {
        let req = Request::builder()
            .method(method)
            .uri(format!("/tedge/file-transfer/{path}"))
            .body(body.into())
            .expect("request builder");

        app.call(req).await.unwrap()
    }

    async fn upload_file(
        app: &mut Router,
        path: &str,
        contents: &str,
    ) -> Response<UnsyncBoxBody<Bytes, axum::Error>> {
        request_with(Method::PUT, app, path, contents.to_owned()).await
    }

    async fn delete_file(
        app: &mut Router,
        path: &str,
    ) -> Response<UnsyncBoxBody<Bytes, axum::Error>> {
        request_with(Method::DELETE, app, path, Body::empty()).await
    }

    async fn download_file(
        app: &mut Router,
        path: &str,
    ) -> Response<UnsyncBoxBody<Bytes, axum::Error>> {
        request_with(Method::GET, app, path, Body::empty()).await
    }

    fn app() -> (TempTedgeDir, Router) {
        let ttd = TempTedgeDir::new();
        let ftd = DataDir::from(ttd.utf8_path_buf()).file_transfer_dir();
        let router = http_file_transfer_router(ftd);
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
