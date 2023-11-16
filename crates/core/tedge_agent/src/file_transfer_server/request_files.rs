use std::fmt;
use std::ops::Deref;
use std::sync::Arc;

use axum::extract::FromRequestParts;
use axum::extract::Path;
use axum::http::request::Parts;
use camino::Utf8Path;
use camino::Utf8PathBuf;

use super::error::FileTransferRequestError;
use super::http_rest::HttpConfig;

/// The paths inferred from a request to the File Transfer Service
pub struct FileTransferPaths {
    /// The full path, i.e. the absolute path on disk the request corresponds to
    pub full: Utf8PathBuf,
    /// The requested path, used to generate error messages, keeping the absolute path encapsulated
    pub request: RequestPath,
}

/// The path from a request, used to generate error messages
///
/// This is a thin wrapper around a [Utf8PathBuf], and is required to create errors
pub struct RequestPath(Utf8PathBuf);

impl Deref for RequestPath {
    type Target = Utf8Path;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<Utf8Path> for RequestPath {
    fn as_ref(&self) -> &Utf8Path {
        &self.0
    }
}

impl fmt::Debug for RequestPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[async_trait::async_trait]
impl FromRequestParts<Arc<HttpConfig>> for FileTransferPaths {
    type Rejection = FileTransferRequestError;

    async fn from_request_parts(
        parts: &mut Parts,
        config: &Arc<HttpConfig>,
    ) -> Result<Self, Self::Rejection> {
        let Path(request_path) = Path::<Utf8PathBuf>::from_request_parts(parts, config).await?;
        local_path_for_file(RequestPath(request_path), config)
    }
}

/// Return the path of the file associated to the given `uri`
///
/// This cleans up the path using [path_clean::clean] and then verifies that this
/// path is actually under `config.file_transfer_dir`
fn local_path_for_file(
    request_path: RequestPath,
    config: &HttpConfig,
) -> Result<FileTransferPaths, FileTransferRequestError> {
    let full_path = config.file_transfer_dir.join(&request_path);

    let clean_path = clean_utf8_path(&full_path);

    if clean_path.starts_with(&config.file_transfer_dir) {
        Ok(FileTransferPaths {
            full: clean_path,
            request: request_path,
        })
    } else {
        Err(FileTransferRequestError::InvalidPath { path: request_path })
    }
}

fn clean_utf8_path(path: &Utf8Path) -> Utf8PathBuf {
    Utf8PathBuf::from(path_clean::clean(path.as_str()))
}
