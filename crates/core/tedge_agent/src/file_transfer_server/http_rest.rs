use crate::file_transfer_server::error::FileTransferError;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use hyper::server::conn::AddrIncoming;
use hyper::Body;
use hyper::Request;
use hyper::Response;
use hyper::Server;
use routerify::Router;
use routerify::RouterService;
use std::net::IpAddr;
use std::net::SocketAddr;
use tedge_actors::futures::StreamExt;
use tedge_config::DEFAULT_DATA_PATH;
use tedge_config::DEFAULT_FILE_TRANSFER_DIR_NAME;
use tedge_utils::paths::create_directories;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;

const HTTP_FILE_TRANSFER_PORT: u16 = 8000;

#[derive(Debug, Clone)]
pub struct HttpConfig {
    pub bind_address: SocketAddr,
    pub file_transfer_uri: String,
    pub data_dir: Utf8PathBuf,
}

impl Default for HttpConfig {
    fn default() -> Self {
        HttpConfig {
            bind_address: ([127, 0, 0, 1], HTTP_FILE_TRANSFER_PORT).into(),
            file_transfer_uri: "/tedge/".into(),
            data_dir: DEFAULT_DATA_PATH.into(),
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

    pub fn with_data_dir(self, data_dir: Utf8PathBuf) -> HttpConfig {
        Self { data_dir, ..self }
    }

    pub fn with_port(self, port: u16) -> HttpConfig {
        let mut bind_address = self.bind_address;
        bind_address.set_port(port);
        Self {
            bind_address,
            ..self
        }
    }

    pub fn file_transfer_end_point(&self) -> String {
        format!("{}file-transfer/*", self.file_transfer_uri)
    }

    pub fn file_transfer_dir_as_string(&self) -> Utf8PathBuf {
        self.data_dir.join(DEFAULT_FILE_TRANSFER_DIR_NAME)
    }

    /// Return the path of the file associated to the given `uri`
    ///
    /// Check that:
    /// * the `uri` is related to the file-transfer i.e a sub-uri of `self.file_transfer_uri`
    /// * the `path`, once normalized, is actually under `self.file_transfer_dir`
    pub fn local_path_for_uri(&self, uri: String) -> Result<Utf8PathBuf, FileTransferError> {
        let ref_uri = uri.clone();

        // The file transfer prefix has to be removed from the uri to get the target path
        let path = uri
            .strip_prefix(&self.file_transfer_uri)
            .ok_or(FileTransferError::InvalidURI { value: ref_uri })?;

        // This path is relative to the file transfer dir
        let full_path = self.data_dir.join(path);

        // One must check that once normalized (i.e. any `..` removed)
        // the path is still under the file transfer dir
        let clean_path = clean_utf8_path(&full_path);
        if clean_path.starts_with(&self.data_dir) {
            Ok(clean_path)
        } else {
            Err(FileTransferError::InvalidURI {
                value: clean_path.to_string(),
            })
        }
    }
}

fn clean_utf8_path(path: &Utf8Path) -> Utf8PathBuf {
    Utf8PathBuf::from(path_clean::clean(path.as_str()))
}

fn separate_path_and_file_name(input: &Utf8Path) -> Option<(Utf8PathBuf, String)> {
    let (relative_path, file_name) = input.as_str().rsplit_once('/')?;

    let relative_path = Utf8PathBuf::from(relative_path);
    Some((relative_path, file_name.into()))
}

async fn put(
    mut request: Request<Body>,
    file_transfer: &HttpConfig,
) -> Result<Response<Body>, FileTransferError> {
    let full_path = file_transfer.local_path_for_uri(request.uri().to_string())?;

    let mut response = Response::new(Body::empty());

    if let Some((relative_path, file_name)) = separate_path_and_file_name(&full_path) {
        let root_path = file_transfer.data_dir.clone();
        let directories_path = root_path.join(relative_path);

        if let Err(_err) = create_directories(&directories_path) {
            *response.status_mut() = hyper::StatusCode::FORBIDDEN;
        }

        let full_path = directories_path.join(file_name);

        match stream_request_body_to_path(&full_path, request.body_mut()).await {
            Ok(()) => {
                *response.status_mut() = hyper::StatusCode::CREATED;
            }
            Err(_err) => {
                *response.status_mut() = hyper::StatusCode::FORBIDDEN;
            }
        }
    } else {
        *response.status_mut() = hyper::StatusCode::FORBIDDEN;
    }
    Ok(response)
}

async fn get(
    request: Request<Body>,
    file_transfer: &HttpConfig,
) -> Result<Response<Body>, FileTransferError> {
    let full_path = file_transfer.local_path_for_uri(request.uri().to_string())?;

    if !full_path.exists() || full_path.is_dir() {
        let mut response = Response::new(Body::empty());
        *response.status_mut() = hyper::StatusCode::NOT_FOUND;
        return Ok(response);
    }

    let mut file = tokio::fs::File::open(full_path).await?;

    let mut contents = vec![];
    file.read_to_end(&mut contents).await?;

    let output = String::from_utf8(contents)?;

    Ok(Response::new(Body::from(output)))
}

async fn delete(
    request: Request<Body>,
    file_transfer: &HttpConfig,
) -> Result<Response<Body>, FileTransferError> {
    let full_path = file_transfer.local_path_for_uri(request.uri().to_string())?;

    let mut response = Response::new(Body::empty());

    if !full_path.exists() {
        *response.status_mut() = hyper::StatusCode::ACCEPTED;
        Ok(response)
    } else {
        match tokio::fs::remove_file(&full_path).await {
            Ok(()) => {
                *response.status_mut() = hyper::StatusCode::ACCEPTED;
                Ok(response)
            }
            Err(_err) => {
                *response.status_mut() = hyper::StatusCode::FORBIDDEN;
                Ok(response)
            }
        }
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
    config: &HttpConfig,
) -> Result<Server<AddrIncoming, RouterService<hyper::Body, FileTransferError>>, FileTransferError>
{
    let file_transfer_end_point = config.file_transfer_end_point();
    let get_config = config.clone();
    let put_config = config.clone();
    let del_config = config.clone();

    let router = Router::builder()
        .get(&file_transfer_end_point, move |req| {
            let config = get_config.clone();
            async move { get(req, &config).await }
        })
        .put(&file_transfer_end_point, move |req| {
            let config = put_config.clone();
            async move { put(req, &config).await }
        })
        .delete(&file_transfer_end_point, move |req| {
            let config = del_config.clone();
            async move { delete(req, &config).await }
        })
        .build()?;
    let router_service = RouterService::new(router)?;

    let server_builder = Server::try_bind(&config.bind_address);
    match server_builder {
        Ok(server) => Ok(server.serve(router_service)),
        Err(_err) => Err(FileTransferError::BindingAddressInUse {
            address: config.bind_address,
        }),
    }
}

#[cfg(test)]
mod test {
    use super::http_file_transfer_server;
    use super::separate_path_and_file_name;
    use crate::file_transfer_server::error::FileTransferError;
    use crate::file_transfer_server::http_rest::HttpConfig;
    use camino::Utf8Path;
    use camino::Utf8PathBuf;
    use hyper::server::conn::AddrIncoming;
    use hyper::Body;
    use hyper::Method;
    use hyper::Request;
    use hyper::Server;
    use routerify::RouterService;
    use tedge_test_utils::fs::TempTedgeDir;
    use test_case::test_case;

    #[test_case(
        "/tedge/some/dir/file",
        Some(Utf8PathBuf::from("/var/tedge/some/dir/file"))
    )]
    #[test_case("/wrong/some/dir/file", None)]
    fn test_remove_prefix_from_uri(input: &str, output: Option<Utf8PathBuf>) {
        let file_transfer = HttpConfig::default();
        let actual_output = file_transfer.local_path_for_uri(input.to_string()).ok();
        assert_eq!(actual_output, output);
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

    const VALID_TEST_URI: &str = "http://127.0.0.1:3000/tedge/file-transfer/another/dir/test-file";
    const INVALID_TEST_URI: &str = "http://127.0.0.1:3000/wrong/place/test-file";

    #[test_case(hyper::Method::GET, VALID_TEST_URI, hyper::StatusCode::OK)]
    #[test_case(hyper::Method::GET, INVALID_TEST_URI, hyper::StatusCode::NOT_FOUND)]
    #[test_case(hyper::Method::DELETE, VALID_TEST_URI, hyper::StatusCode::ACCEPTED)]
    #[test_case(hyper::Method::DELETE, INVALID_TEST_URI, hyper::StatusCode::NOT_FOUND)]
    #[serial_test::serial]
    #[tokio::test]
    async fn test_file_transfer_http_methods(
        method: hyper::Method,
        uri: &'static str,
        status_code: hyper::StatusCode,
    ) {
        let (_ttd, server) = server();
        let client_put_request = client_put_request().await;

        let client_handler = tokio::spawn(async move {
            let client = hyper::Client::new();

            let req = Request::builder()
                .method(method)
                .uri(uri)
                .body(Body::empty())
                .expect("request builder");
            client.request(req).await.unwrap()
        });

        tokio::select! {
            Err(_) = server => {}
            Ok(_put_response) = client_put_request => {
                let response = client_handler.await.unwrap();
                assert_eq!(response.status(), status_code);
            }
        }
    }

    #[test_case(VALID_TEST_URI, hyper::StatusCode::CREATED)]
    #[test_case(INVALID_TEST_URI, hyper::StatusCode::NOT_FOUND)]
    #[serial_test::serial]
    #[tokio::test]
    async fn test_file_transfer_put(uri: &'static str, status_code: hyper::StatusCode) {
        let client_put_request = tokio::spawn(async move {
            let client = hyper::Client::new();

            let mut string = String::new();
            for val in 0..100 {
                string.push_str(&format!("{}\n", val));
            }
            let req = Request::builder()
                .method(Method::PUT)
                .uri(uri)
                .body(Body::from(string.clone()))
                .expect("request builder");

            client.request(req).await.unwrap()
        });

        let (_ttd, server) = server();

        tokio::select! {
            Err(_) = server => {
            }
            Ok(_put_response) = client_put_request => {
                assert_eq!(_put_response.status(), status_code);
            }
        }
    }

    fn server() -> (
        TempTedgeDir,
        Server<AddrIncoming, RouterService<Body, FileTransferError>>,
    ) {
        let ttd = TempTedgeDir::new();
        let tempdir_path = ttd.utf8_path_buf();
        let http_config = HttpConfig::default()
            .with_data_dir(tempdir_path)
            .with_port(3000);
        let server = http_file_transfer_server(&http_config).unwrap();
        (ttd, server)
    }

    // canonicalised client PUT request to create a file in `VALID_TEST_URI`
    // this is to be used to test the GET and DELETE methods.
    async fn client_put_request() -> tokio::task::JoinHandle<hyper::Response<Body>> {
        tokio::spawn(async move {
            let client = hyper::Client::new();

            let string = String::from("file transfer server");

            let req = Request::builder()
                .method(Method::PUT)
                .uri(VALID_TEST_URI)
                .body(Body::from(string.clone()))
                .expect("request builder");
            client.request(req).await.unwrap()
        })
    }

    #[test_case(String::from("/tedge/file-transfer/../../../bin/sh"), false)]
    #[test_case(
        String::from("/tedge/file-transfer/../file-transfer/new/dir/file"),
        true
    )]
    fn test_verify_uri(uri: String, is_ok: bool) {
        let file_transfer = HttpConfig::default();
        let res = file_transfer.local_path_for_uri(uri);
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
