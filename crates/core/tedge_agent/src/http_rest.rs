use futures::StreamExt;
use hyper::{server::conn::AddrIncoming, Body, Request, Response, Server};
use routerify::{Router, RouterService};
use std::path::Path;
use std::{net::IpAddr, net::SocketAddr, path::PathBuf};

use tedge_utils::paths::create_directories;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::error::FileTransferError;

#[derive(Debug, Clone)]
pub struct HttpConfig {
    pub bind_address: SocketAddr,
    pub file_transfer_uri: String,
    pub file_transfer_dir: PathBuf,
}

impl Default for HttpConfig {
    fn default() -> Self {
        HttpConfig {
            bind_address: ([127, 0, 0, 1], 3000).into(),
            file_transfer_uri: "/tedge".into(),
            file_transfer_dir: "/var/tedge".into(),
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

    #[cfg(test)]
    pub fn with_file_transfer_dir(self, file_transfer_dir: PathBuf) -> HttpConfig {
        Self {
            file_transfer_dir,
            ..self
        }
    }

    pub fn file_transfer_end_point(&self) -> String {
        format!("{}/*", self.file_transfer_uri)
    }

    pub fn file_transfer_dir_as_string(&self) -> String {
        self.file_transfer_dir
            .to_str()
            .expect("Non UTF8 http root path")
            .into()
    }
}

mod uri_utils {
    // below are a set of utility function used for working with
    // URI coming from GET, PUT and DELETE requests to the thin-edge
    // device.
    use crate::error::FileTransferError;
    use path_clean::PathClean;
    use std::path::PathBuf;

    pub fn separate_path_and_file_name(input: PathBuf) -> Option<(PathBuf, String)> {
        let input_as_str = input.to_str()?;
        let (relative_path, file_name) = input_as_str.rsplit_once('/')?;

        let relative_path = PathBuf::from(relative_path);
        Some((relative_path, file_name.into()))
    }

    pub fn remove_prefix_from_uri(uri: String) -> Option<String> {
        Some(uri.strip_prefix("/tedge")?.to_string())
    }

    // checks if canonicalised URI path starts_with `FILE_TRANSFER_ROOT_PATH`
    pub fn verify_uri(path: &PathBuf, root_path: &str) -> Result<PathBuf, FileTransferError> {
        let http_transfer_root_path = PathBuf::from(root_path);
        let path = path.clean();
        if path.starts_with(http_transfer_root_path) {
            Ok(path)
        } else {
            Err(FileTransferError::InvalidURI {
                value: path.to_string_lossy().to_string(),
            })
        }
    }
}

async fn put(
    mut request: Request<Body>,
    root_path: &str,
) -> Result<Response<Body>, FileTransferError> {
    let uri = uri_utils::remove_prefix_from_uri(request.uri().to_string()).ok_or(
        FileTransferError::InvalidURI {
            value: request.uri().to_string(),
        },
    )?;

    let mut full_path = PathBuf::from(format!("{}{}", root_path, uri));
    full_path = uri_utils::verify_uri(&full_path, root_path)?;

    let mut response = Response::new(Body::empty());

    if let Some((relative_path, file_name)) = uri_utils::separate_path_and_file_name(full_path) {
        let root_path = PathBuf::from(root_path);
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

async fn get(request: Request<Body>, root_path: &str) -> Result<Response<Body>, FileTransferError> {
    dbg!("get request", &request.uri());
    let uri = uri_utils::remove_prefix_from_uri(request.uri().to_string()).ok_or(
        FileTransferError::InvalidURI {
            value: request.uri().to_string(),
        },
    )?;

    let mut full_path = PathBuf::from(format!("{}{}", root_path, uri));
    dbg!(&full_path);
    full_path = uri_utils::verify_uri(&full_path, root_path)?;
    dbg!(&full_path);

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
    root_path: &str,
) -> Result<Response<Body>, FileTransferError> {
    let uri = uri_utils::remove_prefix_from_uri(request.uri().to_string()).ok_or(
        FileTransferError::InvalidURI {
            value: request.uri().to_string(),
        },
    )?;
    let mut full_path = PathBuf::from(format!("{}{}", root_path, uri));
    full_path = uri_utils::verify_uri(&full_path, root_path)?;

    let mut response = Response::new(Body::empty());

    if !full_path.exists() {
        *response.status_mut() = hyper::StatusCode::INTERNAL_SERVER_ERROR;
        Ok(response)
    } else {
        match tokio::fs::remove_file(&full_path).await {
            Ok(()) => {
                *response.status_mut() = hyper::StatusCode::ACCEPTED;
                Ok(response)
            }
            Err(_err) => {
                *response.status_mut() = hyper::StatusCode::INTERNAL_SERVER_ERROR;
                Ok(response)
            }
        }
    }
}

async fn stream_request_body_to_path(
    path: &Path,
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
    let get_root_path = config.file_transfer_dir_as_string();
    let put_root_path = config.file_transfer_dir_as_string();
    let del_root_path = config.file_transfer_dir_as_string();

    let router = Router::builder()
        .get(&file_transfer_end_point, move |req| {
            let root_path = get_root_path.clone();
            async move { get(req, &root_path).await }
        })
        .put(&file_transfer_end_point, move |req| {
            let root_path = put_root_path.clone();
            async move { put(req, &root_path).await }
        })
        .delete(&file_transfer_end_point, move |req| {
            let root_path = del_root_path.clone();
            async move { delete(req, &root_path).await }
        })
        .build()?;
    let router_service = RouterService::new(router)?;

    Ok(Server::bind(&config.bind_address).serve(router_service))
}

#[cfg(test)]
mod test {

    use std::path::PathBuf;

    use super::{
        http_file_transfer_server,
        uri_utils::{remove_prefix_from_uri, separate_path_and_file_name, verify_uri},
    };
    use crate::error::FileTransferError;
    use crate::http_rest::HttpConfig;
    use hyper::{server::conn::AddrIncoming, Body, Method, Request, Server};
    use routerify::RouterService;
    use tedge_test_utils::fs::TempTedgeDir;
    use test_case::test_case;

    #[test_case("/tedge/some/dir/file", Some(String::from("/some/dir/file")))]
    #[test_case("/wrong/some/dir/file", None)]
    fn test_remove_prefix_from_uri(input: &str, output: Option<String>) {
        let actual_output = remove_prefix_from_uri(input.to_string());
        assert_eq!(actual_output, output);
    }

    #[test_case("/tedge/some/dir/file", "/tedge/some/dir", "file")]
    #[test_case("/tedge/some/dir/", "/tedge/some/dir", "")]
    fn test_separate_path_and_file_name(
        input: &str,
        expected_path: &str,
        expected_file_name: &str,
    ) {
        let (actual_path, actual_file_name) = separate_path_and_file_name(input.into()).unwrap();
        assert_eq!(actual_path, std::path::PathBuf::from(expected_path));
        assert_eq!(actual_file_name, expected_file_name);
    }

    const VALID_TEST_URI: &'static str = "http://127.0.0.1:3000/tedge/another/dir/test-file";
    const INVALID_TEST_URI: &'static str = "http://127.0.0.1:3000/wrong/place/test-file";

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
    #[test_case(INVALID_TEST_URI, hyper::StatusCode::NOT_FOUND)] // TODO what about 403
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
            let response = client.request(req).await.unwrap();
            response
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
        let tempdir_path = ttd.path().to_owned();
        let http_config = HttpConfig::default().with_file_transfer_dir(tempdir_path);
        let server = http_file_transfer_server(&http_config).unwrap();
        (ttd, server)
    }

    // canonicalised client PUT request to create a file in `VALID_TEST_URI`
    // this is to be used to test the GET and DELETE methods.
    async fn client_put_request() -> tokio::task::JoinHandle<hyper::Response<Body>> {
        let handle = tokio::spawn(async move {
            let client = hyper::Client::new();

            let string = String::from("file transfer server");

            let req = Request::builder()
                .method(Method::PUT)
                .uri(VALID_TEST_URI)
                .body(Body::from(string.clone()))
                .expect("request builder");
            client.request(req).await.unwrap()
        });
        handle
    }
    #[test_case(
        PathBuf::from("/var/tedge/file-transfer/../../../bin/sh"),
        "/var/tedge/file-transfer",
        false
    )]
    #[test_case(
        PathBuf::from("/var/tedge/file-transfer/../file-transfer/new/dir/file"),
        "/var/tedge/file-transfer",
        true
    )]
    fn test_verify_uri(path: PathBuf, root_path: &str, is_ok: bool) {
        let res = verify_uri(&path, root_path);
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
