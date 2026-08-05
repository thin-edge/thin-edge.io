use crate::error::UploadError;
use backoff::future::retry_notify;
use backoff::ExponentialBackoff;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use certificate::http_client;
use certificate::CloudHttpConfig;
use mime::Mime;
use mime_guess::MimeGuess;
use reqwest::header::CONTENT_LENGTH;
use reqwest::header::CONTENT_TYPE;
use reqwest::multipart;
use reqwest::Body;
use reqwest::Identity;
use reqwest::StatusCode;
use std::time::Duration;
use tokio::fs::File;
use tokio_util::codec::BytesCodec;
use tokio_util::codec::FramedRead;
use tracing::debug;
use tracing::info;
use tracing::warn;

/// Path prefixes for which retry for status 500 is disabled. This is to avoid retrying for e.g.
/// File Transfer Service, for which we currently test that on 500 we shouldn't retry.
///
/// The optimal solution would be for components interacting with the FTS to call a wrapper API
/// which would handle the exceptions from the generic HTTP client, but for now we disable retry for
/// these paths. This is not ideal because these paths could theoretically be used by some other,
/// possibly remote services, but it's acceptable for now as a workaround.
const DISABLED_500_RETRY_PATHS: [&str; 2] = ["/te/v1/files/", "/tedge/file-transfer/"];

fn is_fts_path(path: &str) -> bool {
    DISABLED_500_RETRY_PATHS.iter().any(|p| path.starts_with(p))
}

fn default_backoff() -> ExponentialBackoff {
    // Default retry is an exponential retry with a limit of 5 minutes total.
    // Let's set some more reasonable retry policy so we don't block the uploads for too long.
    ExponentialBackoff {
        initial_interval: Duration::from_secs(15),
        max_elapsed_time: Some(Duration::from_secs(300)),
        randomization_factor: 0.1,
        ..Default::default()
    }
}

/// Auto tries to detect the mime from the given file extension without direct file access.
///
/// Custom sets a custom Content-Type.
/// If multipart request is needed, choose FormData.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ContentType {
    Auto,
    Custom(Mime),
    FormData(FormData),
}

/// Dataset to construct reqwest::multipart::Form.
///
/// To avoid using reqwest::multipart::Form inside the ContentType enum
/// since reqwest::multipart::Form doesn't support Copy or Clone.
/// If mime is None, the mime will be guessed while uploading a file.
#[derive(Debug, Eq, Clone, PartialEq)]
pub struct FormData {
    filename: String,
    mime: Option<Mime>,
}

impl FormData {
    pub fn new(filename: String) -> Self {
        Self {
            filename,
            mime: None,
        }
    }

    pub fn set_mime(self, mime: Mime) -> Self {
        Self {
            mime: Some(mime),
            ..self
        }
    }

    pub fn text_plain(self) -> Self {
        self.set_mime(mime::TEXT_PLAIN)
    }
}

/// Switch upload method
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum UploadMethod {
    PUT,
    POST,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct UploadInfo {
    pub url: String,
    pub auth: Option<Auth>,
    pub content_type: ContentType,
    pub method: UploadMethod,
}

impl From<&str> for UploadInfo {
    fn from(url: &str) -> Self {
        Self::new(url)
    }
}

impl UploadInfo {
    pub fn new(url: &str) -> Self {
        Self {
            url: url.into(),
            auth: None,
            content_type: ContentType::Auto,
            method: UploadMethod::PUT,
        }
    }

    pub fn with_auth(self, auth: Auth) -> Self {
        Self {
            auth: Some(auth),
            ..self
        }
    }

    pub fn set_content_type(self, content_type: ContentType) -> Self {
        Self {
            content_type,
            ..self
        }
    }

    pub fn set_method(self, method: UploadMethod) -> Self {
        Self { method, ..self }
    }

    pub fn url(&self) -> &str {
        self.url.as_str()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Auth {
    Bearer(String),
}

#[derive(Debug)]
pub struct Uploader {
    source_filename: Utf8PathBuf,
    backoff: ExponentialBackoff,
    identity: Option<Identity>,
    cloud_http_config: CloudHttpConfig,
}

impl Uploader {
    pub fn new(
        target_path: Utf8PathBuf,
        identity: Option<Identity>,
        cloud_root_certs: CloudHttpConfig,
    ) -> Self {
        Self {
            source_filename: target_path,
            backoff: default_backoff(),
            identity,
            cloud_http_config: cloud_root_certs,
        }
    }

    pub fn set_backoff(&mut self, backoff: ExponentialBackoff) {
        self.backoff = backoff;
    }

    pub async fn upload(&self, url: &UploadInfo) -> Result<(), UploadError> {
        self.upload_request(url).await?;

        Ok(())
    }

    async fn upload_request(&self, url: &UploadInfo) -> Result<reqwest::Response, UploadError> {
        use crate::error::ErrContext;

        let operation = || async {
            let file = File::open(&self.source_filename)
                .await
                .context(format!("Can't open a file {:?}", self.source_filename))
                .map_err(backoff::Error::Permanent)?;

            let file_length = file
                .metadata()
                .await
                .context(format!(
                    "Can't read a file {:?} metadata",
                    self.source_filename
                ))
                .map_err(backoff::Error::Permanent)?
                .len();

            let file_body = Body::wrap_stream(FramedRead::new(file, BytesCodec::new()));

            let mut client = self.cloud_http_config.client_builder();
            if let Some(identity) = self.identity.clone() {
                client = client.identity(identity);
            }
            let client = client
                .build()
                .map_err(UploadError::from)
                .map_err(backoff::Error::Permanent)?;

            // If HTTPS is enabled for the file transfer service, the response to an HTTP request
            // will be a temporary redirect. We can't retry the PUT request, so we first perform a
            // HEAD request to establish the correct URL
            let head_res = client.head(url.url()).send().await;
            let head_res_url = match &head_res {
                Ok(res) => Some(res.url()),
                Err(err) => {
                    // e.g. if we need a client certificate but haven't provided one
                    // We handle this error here because if there is a certificate error now
                    // there is guaranteed to be one later
                    if axum_tls::rustls_error_from_reqwest(err).is_some() {
                        return Err(backoff::Error::Permanent(head_res.unwrap_err().into()));
                    }
                    err.url()
                }
            };
            let target_url = head_res_url.map_or(url.url(), |u| u.as_str());

            if target_url != url.url() {
                info!("Redirecting request from {} to {target_url}", url.url())
            }

            let mut client = match url.method {
                UploadMethod::PUT => client.put(target_url),
                UploadMethod::POST => client.post(target_url),
            };

            if let Some(Auth::Bearer(token)) = &url.auth {
                client = client.bearer_auth(token)
            }

            client = match url.content_type.clone() {
                ContentType::Auto => {
                    let mime = MimeGuess::from_path(&self.source_filename).first_or_octet_stream();
                    client
                        .header(CONTENT_TYPE, mime.as_ref())
                        .header(CONTENT_LENGTH, file_length)
                        .body(file_body)
                }
                ContentType::Custom(mime) => client
                    .header(CONTENT_TYPE, mime.as_ref())
                    .header(CONTENT_LENGTH, file_length)
                    .body(file_body),
                ContentType::FormData(data) => {
                    let mime = match data.mime {
                        None => MimeGuess::from_path(&self.source_filename).first_or_octet_stream(),
                        Some(mime) => mime,
                    };
                    let file_part = multipart::Part::stream_with_length(file_body, file_length)
                        .file_name(data.filename)
                        .mime_str(mime.as_ref())
                        .unwrap(); // safe, ensured that mime is valid
                    let form = multipart::Form::new().part("file", file_part);
                    client.multipart(form)
                }
            };

            client
                .send()
                .await
                .map_err(|err| {
                    if err.is_builder() || err.is_connect() {
                        backoff::Error::Permanent(UploadError::Network(err))
                    } else {
                        backoff::Error::transient(UploadError::Network(err))
                    }
                })?
                .error_for_status()
                .map_err(|err| {
                    let path = err.url().map(|url| url.path()).unwrap_or_default();
                    match err.status() {
                        Some(StatusCode::INTERNAL_SERVER_ERROR) if is_fts_path(path) => {
                            debug!(
                                "Path '{path}' is in the list of paths for which retry for status 500 is disabled, so not retrying"
                            );
                            backoff::Error::permanent(UploadError::Network(err))
                        }
                        Some(status) => {
                            if http_client::is_status_retryable(status) {
                                backoff::Error::transient(UploadError::Network(err))
                            } else {
                                backoff::Error::permanent(UploadError::Network(err))
                            }
                        }
                        _ => backoff::Error::transient(UploadError::Network(err)),
                    }
                })
        };

        retry_notify(self.backoff.clone(), operation, |err, dur: Duration| {
            let dur = dur.as_secs();
            warn!("Temporary failure: {err}. Retrying in {dur}s",)
        })
        .await
    }

    pub fn filename(&self) -> &Utf8Path {
        self.source_filename.as_path()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use axum::routing::put;
    use axum::Router;
    use backoff::ExponentialBackoffBuilder;
    use futures::future::pending;
    use futures::stream::StreamExt;
    use std::future::IntoFuture as _;
    use std::sync::atomic::AtomicBool;
    use std::sync::atomic::Ordering;
    use std::sync::Arc;
    use tedge_test_utils::fs::TempTedgeDir;
    use tempfile::tempdir;
    use tokio::fs::read_to_string;
    use tokio::io::AsyncWriteExt;
    use tokio::io::BufWriter;
    use tokio::net::TcpListener;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn upload_has_user_agent() {
        let mut server = mockito::Server::new_async().await;
        let _mock1 = server
            .mock("PUT", "/some_file.txt")
            .with_status(201)
            .match_header("user-agent", certificate::http_client::USER_AGENT)
            .with_body("hello")
            .create();

        let mut target_url = server.url();
        target_url.push_str("/some_file.txt");

        let url = UploadInfo::new(&target_url);

        let ttd = TempTedgeDir::new();
        ttd.file("file_upload.txt")
            .with_raw_content("Hello, world!");

        let mut uploader = Uploader::new(
            ttd.utf8_path().join("file_upload.txt"),
            None,
            CloudHttpConfig::test_value(),
        );
        uploader.set_backoff(ExponentialBackoff {
            current_interval: Duration::ZERO,
            max_elapsed_time: Some(Duration::ZERO),
            ..Default::default()
        });

        assert!(uploader.upload(&url).await.is_ok())
    }

    #[tokio::test]
    async fn upload_content_no_auth() {
        let mut server = mockito::Server::new_async().await;
        let _mock1 = server
            .mock("PUT", "/some_file.txt")
            .with_status(201)
            .create();

        let mut target_url = server.url();
        target_url.push_str("/some_file.txt");

        let url = UploadInfo::new(&target_url);

        let ttd = TempTedgeDir::new();
        ttd.file("file_upload.txt")
            .with_raw_content("Hello, world!");

        let mut uploader = Uploader::new(
            ttd.utf8_path().join("file_upload.txt"),
            None,
            CloudHttpConfig::test_value(),
        );
        uploader.set_backoff(ExponentialBackoff {
            current_interval: Duration::ZERO,
            ..Default::default()
        });

        assert!(uploader.upload(&url).await.is_ok())
    }

    #[tokio::test]
    async fn upload_content_no_auth_post() {
        let mut server = mockito::Server::new_async().await;
        let _mock1 = server
            .mock("POST", "/some_file.txt")
            .with_status(201)
            .create();

        let mut target_url = server.url();
        target_url.push_str("/some_file.txt");

        let url = UploadInfo::new(&target_url)
            .set_content_type(ContentType::FormData(FormData::new("filename".into())))
            .set_method(UploadMethod::POST);

        let ttd = TempTedgeDir::new();
        ttd.file("file_upload.txt")
            .with_raw_content("Hello, world!");

        let mut uploader = Uploader::new(
            ttd.utf8_path().join("file_upload.txt"),
            None,
            CloudHttpConfig::test_value(),
        );
        uploader.set_backoff(ExponentialBackoff {
            current_interval: Duration::ZERO,
            ..Default::default()
        });

        assert!(uploader.upload(&url).await.is_ok())
    }

    #[tokio::test]
    async fn upload_content_with_auth() {
        let mut server = mockito::Server::new_async().await;
        let _mock1 = server
            .mock("PUT", "/some_file.txt")
            .with_status(201)
            .match_header(
                "Authorization",
                mockito::Matcher::Regex(r"Bearer .*".to_string()),
            )
            .create();

        let mut target_url = server.url();
        target_url.push_str("/some_file.txt");

        let url = UploadInfo::new(&target_url).with_auth(Auth::Bearer("1234".to_string()));

        let ttd = TempTedgeDir::new();
        ttd.file("file_upload.txt")
            .with_raw_content("Hello, world!");

        let mut uploader = Uploader::new(
            ttd.utf8_path().join("file_upload.txt"),
            None,
            CloudHttpConfig::test_value(),
        );

        uploader.set_backoff(ExponentialBackoff {
            current_interval: Duration::ZERO,
            ..Default::default()
        });

        assert!(uploader.upload(&url).await.is_ok())
    }

    #[tokio::test]
    async fn upload_content_from_file_that_does_not_exist() {
        let mut server = mockito::Server::new_async().await;
        let _mock1 = server
            .mock("PUT", "/some_file.txt")
            .with_status(201)
            .create();

        let mut target_url = server.url();
        target_url.push_str("/some_file.txt");

        let url = UploadInfo::new(&target_url);

        // Not existing filename
        let source_path = Utf8Path::new("not_exist.txt").to_path_buf();

        let uploader = Uploader::new(source_path, None, CloudHttpConfig::test_value());
        assert!(uploader.upload(&url).await.is_err());
    }

    #[test]
    fn default_uploader_uses_customised_backoff_parameters() {
        let uploader = Uploader::new(Utf8PathBuf::default(), None, CloudHttpConfig::test_value());

        assert_eq!(uploader.backoff.initial_interval, Duration::from_secs(15));
        assert_eq!(
            uploader.backoff.max_elapsed_time,
            Some(Duration::from_secs(300))
        );
        assert_eq!(uploader.backoff.randomization_factor, 0.1);
    }

    #[tokio::test]
    async fn retry_upload_when_disconnected() {
        use anyhow::Context;
        let temp_dir = Arc::new(tempdir().unwrap());

        let listener = TcpListener::bind("localhost:0").await.unwrap();

        let port = listener.local_addr().unwrap().port();

        let target_path = Arc::new(
            Utf8Path::from_path(temp_dir.path())
                .unwrap()
                .join("target.txt"),
        );
        let target_path_clone = target_path.clone();
        let is_first_attempt = Arc::new(AtomicBool::new(true));
        let (io_err_tx, mut io_err_rx) = mpsc::channel::<anyhow::Error>(1);

        let app = Router::new().route(
            "/target.txt",
            put(|body: axum::body::Body| async move {
                let res = async {
                    if is_first_attempt.fetch_and(false, Ordering::SeqCst) {
                        Ok(StatusCode::SERVICE_UNAVAILABLE)
                    } else {
                        let mut file = BufWriter::new(
                            File::create(target_path_clone.as_path())
                                .await
                                .context("creating file")?,
                        );
                        let mut body_stream = body.into_data_stream();
                        while let Some(chunk) = body_stream.next().await {
                            file.write_all(&chunk.context("receiving chunk")?)
                                .await
                                .context("writing chunk")?;
                        }
                        file.flush().await.context("flushing buffer of file")?;
                        Ok(StatusCode::CREATED)
                    }
                }
                .await;

                match res {
                    Ok(status_code) => status_code,
                    Err(err) => {
                        io_err_tx.send(err).await.unwrap();
                        // If we've encountered a server error, don't respond
                        // The uploader will keep running, so the main task will see the error
                        // message on the channel and panic accordingly
                        pending().await
                    }
                }
            }),
        );

        let server_task = tokio::spawn(axum::serve(listener, app).into_future());

        tokio::time::sleep(Duration::from_millis(50)).await;

        let source_path = Utf8Path::from_path(temp_dir.path())
            .unwrap()
            .join("source.txt");

        let mut source_file = File::create(&source_path).await.unwrap();

        write_to_file_with_size(&mut source_file, 1024 * 1024).await;

        let mut uploader =
            Uploader::new(source_path.to_owned(), None, CloudHttpConfig::test_value());
        // Adjust the backoff to be super fast for testing purposes
        uploader.set_backoff(
            ExponentialBackoffBuilder::new()
                .with_initial_interval(Duration::from_millis(10))
                .with_max_elapsed_time(Some(Duration::from_secs(10)))
                .build(),
        );
        let url = UploadInfo::new(&format!("http://localhost:{port}/target.txt"));

        tokio::select! {
            upload_res = uploader.upload(&url) => upload_res.unwrap(),
            server_err = io_err_rx.recv() => panic!("{:?}", server_err),
        };

        server_task.abort();

        let target_content = read_to_string(target_path.as_path()).await.unwrap();
        let source_content = read_to_string(source_path).await.unwrap();

        assert_eq!(source_content.len(), target_content.len());
        assert_eq!(source_content, target_content);
    }

    #[tokio::test]
    async fn should_retry_for_statuses() {
        let retryable = [
            StatusCode::REQUEST_TIMEOUT,
            StatusCode::TOO_EARLY,
            StatusCode::TOO_MANY_REQUESTS,
            StatusCode::INTERNAL_SERVER_ERROR,
            StatusCode::BAD_GATEWAY,
            StatusCode::SERVICE_UNAVAILABLE,
            StatusCode::GATEWAY_TIMEOUT,
        ];

        let non_retryable = [
            // 4xx
            StatusCode::BAD_REQUEST,
            StatusCode::UNAUTHORIZED,
            StatusCode::PAYMENT_REQUIRED,
            StatusCode::FORBIDDEN,
            StatusCode::NOT_FOUND,
            StatusCode::METHOD_NOT_ALLOWED,
            StatusCode::NOT_ACCEPTABLE,
            StatusCode::PROXY_AUTHENTICATION_REQUIRED,
            StatusCode::CONFLICT,
            StatusCode::GONE,
            StatusCode::LENGTH_REQUIRED,
            StatusCode::PRECONDITION_FAILED,
            StatusCode::PAYLOAD_TOO_LARGE,
            StatusCode::URI_TOO_LONG,
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            StatusCode::RANGE_NOT_SATISFIABLE,
            StatusCode::EXPECTATION_FAILED,
            StatusCode::IM_A_TEAPOT,
            StatusCode::MISDIRECTED_REQUEST,
            StatusCode::UNPROCESSABLE_ENTITY,
            StatusCode::LOCKED,
            StatusCode::FAILED_DEPENDENCY,
            StatusCode::UPGRADE_REQUIRED,
            StatusCode::PRECONDITION_REQUIRED,
            StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE,
            StatusCode::UNAVAILABLE_FOR_LEGAL_REASONS,
            // 5xx
            StatusCode::NOT_IMPLEMENTED,
            StatusCode::HTTP_VERSION_NOT_SUPPORTED,
            StatusCode::VARIANT_ALSO_NEGOTIATES,
            StatusCode::INSUFFICIENT_STORAGE,
            StatusCode::LOOP_DETECTED,
            StatusCode::NOT_EXTENDED,
            StatusCode::NETWORK_AUTHENTICATION_REQUIRED,
        ];
        let statuses = retryable.into_iter().chain(non_retryable);

        let mut server = mockito::Server::new_async().await;
        for status in statuses {
            let mock = server
                .mock("PUT", format!("/{}", status.as_u16()).as_str())
                .with_status(status.as_u16().into());
            let mock = if retryable.contains(&status) {
                mock.expect(2)
            } else {
                mock.expect(1)
            };
            let mock = mock.create_async().await;

            let res = attempt_upload(&server, format!("/{}", status.as_u16()).as_str()).await;
            assert!(matches!(res, Err(UploadError::Network(_))), "{res:?}");

            mock.assert_async().await;
        }
    }

    #[tokio::test]
    async fn should_not_retry_for_fts_500() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("PUT", "/te/v1/files/test")
            .with_status(500)
            .expect(1)
            .create_async()
            .await;
        let UploadError::Network(err) = attempt_upload(&server, "/te/v1/files/test")
            .await
            .unwrap_err()
        else {
            panic!("should be 500")
        };
        assert_eq!(err.status().unwrap(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    async fn attempt_upload(
        server: &mockito::ServerGuard,
        resource: &str,
    ) -> Result<(), UploadError> {
        let ttd = TempTedgeDir::new();
        let file = ttd
            .file("file_upload.txt")
            .with_raw_content("Hello, world!");

        let mut target_url = server.url();
        target_url.push_str(resource);

        let url = UploadInfo::new(&target_url).set_method(UploadMethod::PUT);

        let mut uploader = Uploader::new(file.utf8_path_buf(), None, CloudHttpConfig::test_value());
        // tweaked to exactly 1 retry
        uploader.set_backoff(ExponentialBackoff {
            initial_interval: Duration::from_millis(5),
            multiplier: 10.0,
            randomization_factor: f64::EPSILON,
            max_elapsed_time: Some(Duration::from_millis(50)),
            ..Default::default()
        });
        uploader.upload(&url).await
    }

    async fn write_to_file_with_size(file: &mut File, size: usize) {
        let data: String = "Some data!".into();
        let loops = size / data.len();
        let mut buffer = String::with_capacity(size);
        for _ in 0..loops {
            buffer.push_str("Some data!");
        }

        file.write_all(buffer.as_bytes()).await.unwrap();
        file.flush().await.unwrap();
    }
}
