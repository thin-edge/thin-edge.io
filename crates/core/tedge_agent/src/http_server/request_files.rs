use std::fmt;
use std::ops::Deref;
use std::sync::Arc;

use axum::extract::FromRequestParts;
use axum::extract::Path;
use axum::http::request::Parts;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use tedge_utils::paths::ManagedDir;
use tedge_utils::paths::TedgePaths;

use super::error::HttpRequestError;

#[derive(Clone)]
pub(super) struct FileTransferDir {
    file_transfer_dir: Arc<ManagedDir>,
    data_dir: Arc<TedgePaths>,
}

impl FileTransferDir {
    pub(super) fn new(file_transfer_dir: ManagedDir, data_dir: TedgePaths) -> Self {
        Self {
            file_transfer_dir: Arc::new(file_transfer_dir),
            data_dir: Arc::new(data_dir),
        }
    }
}

/// The paths inferred from a request to the File Transfer Service
pub struct FileTransferPath {
    /// The full path, i.e. the absolute path on disk the request corresponds to
    pub full: Utf8PathBuf,
    /// The requested path, used to generate error messages, keeping the absolute path encapsulated
    pub request: RequestPath,
    /// The data root (e.g. `/var/tedge`), used to create parent directories with the correct root on upload
    pub data_dir: Arc<TedgePaths>,
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

impl FromRequestParts<FileTransferDir> for FileTransferPath {
    type Rejection = HttpRequestError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &FileTransferDir,
    ) -> Result<Self, Self::Rejection> {
        let Path(request_path) = Path::<Utf8PathBuf>::from_request_parts(parts, state).await?;
        local_path_for_file(
            RequestPath(request_path),
            state.file_transfer_dir.clone(),
            state.data_dir.clone(),
        )
    }
}

/// Return the path of the file associated to the given `uri`
///
/// This cleans up the path using [path_clean::clean] and then verifies that this
/// path is actually under `file_transfer_dir`
fn local_path_for_file(
    request_path: RequestPath,
    file_transfer_dir: Arc<ManagedDir>,
    data_dir: Arc<TedgePaths>,
) -> Result<FileTransferPath, HttpRequestError> {
    let full_path = file_transfer_dir.path().join(&request_path);

    let clean_path = clean_utf8_path(&full_path);

    if clean_path.starts_with(file_transfer_dir.path()) {
        Ok(FileTransferPath {
            full: clean_path,
            request: request_path,
            data_dir,
        })
    } else {
        Err(HttpRequestError::InvalidPath { path: request_path })
    }
}

fn clean_utf8_path(path: &Utf8Path) -> Utf8PathBuf {
    // unwrap is safe because clean returns an utf8 path when given an utf8 path
    Utf8PathBuf::try_from(path_clean::clean(path.as_std_path())).unwrap()
}
