use async_trait::async_trait;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use log::info;
use tedge_actors::Sequential;
use tedge_actors::Server;
use tedge_actors::ServerActorBuilder;
use tedge_actors::ServerConfig;
use upload::Auth;
use upload::UploadError;
use upload::UploadInfo;
use upload::Uploader;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct UploadRequest {
    pub url: String,
    pub file_path: Utf8PathBuf,
    pub auth: Option<Auth>,
}

impl UploadRequest {
    pub fn new(url: &str, file_path: &Utf8Path) -> Self {
        Self {
            url: url.into(),
            file_path: file_path.to_owned(),
            auth: None,
        }
    }

    pub fn with_auth(self, auth: Auth) -> Self {
        Self {
            auth: Some(auth),
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

#[derive(Debug, Default)]
pub struct UploaderActor {
    config: ServerConfig,
}

impl UploaderActor {
    pub fn new() -> Self {
        UploaderActor::default()
    }

    pub fn builder(&self) -> ServerActorBuilder<UploaderActor, Sequential> {
        ServerActorBuilder::new(UploaderActor::default(), &ServerConfig::new(), Sequential)
    }

    pub fn with_capacity(self, capacity: usize) -> Self {
        Self {
            config: self.config.with_capacity(capacity),
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

        let upload_info = if let Some(auth) = request.auth {
            UploadInfo::new(&request.url).with_auth(auth)
        } else {
            UploadInfo::new(&request.url)
        };

        let uploader = Uploader::new(request.file_path.clone());

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
