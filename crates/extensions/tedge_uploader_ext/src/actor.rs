use async_trait::async_trait;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use log::info;
use reqwest::Identity;
use tedge_actors::Sequential;
use tedge_actors::Server;
use tedge_actors::ServerActorBuilder;
use tedge_actors::ServerConfig;
use tedge_utils::certificates::CloudRootCerts;
use upload::Auth;
use upload::ContentType;
use upload::UploadError;
use upload::UploadInfo;
use upload::UploadMethod;
use upload::Uploader;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct UploadRequest {
    pub url: String,
    pub file_path: Utf8PathBuf,
    pub auth: Option<Auth>,
    pub content_type: ContentType,
    pub method: UploadMethod,
}

impl UploadRequest {
    pub fn new(url: &str, file_path: &Utf8Path) -> Self {
        Self {
            url: url.into(),
            file_path: file_path.to_owned(),
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

    pub fn with_content_type(self, content_type: ContentType) -> Self {
        Self {
            content_type,
            ..self
        }
    }

    pub fn put(self) -> Self {
        Self {
            method: UploadMethod::PUT,
            ..self
        }
    }

    pub fn post(self) -> Self {
        Self {
            method: UploadMethod::POST,
            ..self
        }
    }
}

#[derive(Debug)]
pub struct UploadResponse {
    pub url: String,
    pub file_path: Utf8PathBuf,
}

impl UploadResponse {
    pub fn new(url: &str, file_path: Utf8PathBuf) -> Self {
        Self {
            url: url.into(),
            file_path,
        }
    }
}

pub type UploadResult = Result<UploadResponse, UploadError>;

#[derive(Debug)]
pub struct UploaderActor {
    config: ServerConfig,
    identity: Option<Identity>,
    cloud_root_certs: CloudRootCerts,
}

impl UploaderActor {
    pub fn new(identity: Option<Identity>, cloud_root_certs: CloudRootCerts) -> Self {
        Self {
            config: ServerConfig::default(),
            identity,
            cloud_root_certs,
        }
    }
    pub fn builder(self) -> ServerActorBuilder<UploaderActor, Sequential> {
        let config = self.config;
        ServerActorBuilder::new(self, &config, Sequential)
    }

    pub fn with_capacity(self, capacity: usize) -> Self {
        Self {
            config: self.config.with_capacity(capacity),
            ..self
        }
    }
}

#[async_trait]
impl Server for UploaderActor {
    type Request = (String, UploadRequest);
    type Response = (String, UploadResult);

    fn name(&self) -> &str {
        "Uploader"
    }

    async fn handle(&mut self, id_request: Self::Request) -> Self::Response {
        let (id, request) = id_request;

        let mut upload_info = UploadInfo::new(&request.url)
            .set_content_type(request.content_type)
            .set_method(request.method);
        if let Some(auth) = request.auth {
            upload_info = upload_info.with_auth(auth);
        }

        let uploader = Uploader::new(
            request.file_path.clone(),
            self.identity.clone(),
            self.cloud_root_certs.clone(),
        );

        info!(
            "Uploading from {} to url: {}",
            request.file_path, request.url,
        );

        let result = match uploader.upload(&upload_info).await {
            Ok(_) => Ok(UploadResponse::new(
                request.url.as_str(),
                uploader.filename().to_path_buf(),
            )),
            Err(err) => Err(err),
        };

        (id, result)
    }
}
