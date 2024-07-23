use async_trait::async_trait;
use download::Auth;
use download::DownloadError;
use download::DownloadInfo;
use download::Downloader;
use log::info;
use reqwest::Identity;
use std::marker::PhantomData;
use std::path::Path;
use std::path::PathBuf;
use tedge_actors::Message;
use tedge_actors::Sequential;
use tedge_actors::Server;
use tedge_actors::ServerActorBuilder;
use tedge_actors::ServerConfig;
use certificate::CloudRootCerts;
use tedge_utils::file::PermissionEntry;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DownloadRequest {
    pub url: String,
    pub file_path: PathBuf,
    pub auth: Option<Auth>,
    pub permission: Option<PermissionEntry>,
}

impl DownloadRequest {
    pub fn new(url: &str, file_path: &Path) -> Self {
        Self {
            url: url.into(),
            file_path: file_path.into(),
            auth: None,
            permission: None,
        }
    }

    pub fn with_auth(self, auth: Auth) -> Self {
        Self {
            auth: Some(auth),
            ..self
        }
    }

    pub fn with_permission(self, permission: PermissionEntry) -> Self {
        Self {
            permission: Some(permission),
            ..self
        }
    }
}

pub type DownloadResult = Result<DownloadResponse, DownloadError>;

#[derive(Debug)]
pub struct DownloadResponse {
    pub url: String,
    pub file_path: PathBuf,
}

impl DownloadResponse {
    pub fn new(url: &str, file_path: &Path) -> Self {
        Self {
            url: url.into(),
            file_path: file_path.into(),
        }
    }
}

#[derive(Debug)]
pub struct DownloaderActor<T> {
    config: ServerConfig,
    key: std::marker::PhantomData<T>,
    identity: Option<Identity>,
    cloud_root_certs: CloudRootCerts,
}

impl<T> Clone for DownloaderActor<T> {
    fn clone(&self) -> Self {
        DownloaderActor {
            config: self.config,
            key: self.key,
            identity: self.identity.clone(),
            cloud_root_certs: self.cloud_root_certs.clone(),
        }
    }
}

impl<T: Message + Default> DownloaderActor<T> {
    pub fn new(identity: Option<Identity>, cloud_root_certs: CloudRootCerts) -> Self {
        DownloaderActor {
            config: <_>::default(),
            key: PhantomData,
            identity,
            cloud_root_certs,
        }
    }

    pub fn builder(&self) -> ServerActorBuilder<DownloaderActor<T>, Sequential> {
        ServerActorBuilder::new(self.clone(), &ServerConfig::new(), Sequential)
    }

    pub fn with_capacity(self, capacity: usize) -> Self {
        Self {
            config: self.config.with_capacity(capacity),
            ..self
        }
    }
}

#[async_trait]
impl<T: Message> Server for DownloaderActor<T> {
    type Request = (T, DownloadRequest);
    type Response = (T, DownloadResult);

    fn name(&self) -> &str {
        "Downloader"
    }

    async fn handle(&mut self, id_request: Self::Request) -> Self::Response {
        let (id, request) = id_request;

        let download_info = if let Some(auth) = request.auth {
            DownloadInfo::new(&request.url).with_auth(auth)
        } else {
            DownloadInfo::new(&request.url)
        };

        let downloader = if let Some(permission) = request.permission {
            Downloader::with_permission(
                request.file_path.clone(),
                permission,
                self.identity.clone(),
                self.cloud_root_certs.clone(),
            )
        } else {
            Downloader::new(
                request.file_path.clone(),
                self.identity.clone(),
                self.cloud_root_certs.clone(),
            )
        };

        info!(
            "Downloading from url {} to location {}",
            request.url,
            request.file_path.display()
        );

        let result = match downloader.download(&download_info).await {
            Ok(_) => Ok(DownloadResponse::new(
                request.url.as_str(),
                downloader.filename(),
            )),
            Err(err) => Err(err),
        };

        (id, result)
    }
}
