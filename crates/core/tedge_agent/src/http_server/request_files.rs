use std::fmt;
use std::ops::Deref;
use std::sync::Arc;

use axum::extract::FromRequestParts;
use axum::extract::Path;
use axum::http::request::Parts;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use tedge_api::path::DataDir;
use tedge_utils::paths::ManagedDir;

use super::error::HttpRequestError;

#[derive(Clone)]
pub(super) struct FileTransferDir {
    file_transfer_dir: Arc<ManagedDir>,
    data_dir: Arc<DataDir>,
}

impl FileTransferDir {
    pub(super) fn new(file_transfer_dir: ManagedDir, data_dir: DataDir) -> Self {
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
    pub data_dir: Arc<DataDir>,
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
/// path is actually under `file_transfer_dir`, both lexically and once symlinks
/// have been resolved.
fn local_path_for_file(
    request_path: RequestPath,
    file_transfer_dir: Arc<ManagedDir>,
    data_dir: Arc<DataDir>,
) -> Result<FileTransferPath, HttpRequestError> {
    let full_path = file_transfer_dir.path().join(&request_path);

    let clean_path = clean_utf8_path(&full_path);

    if !clean_path.starts_with(file_transfer_dir.path()) {
        return Err(HttpRequestError::InvalidPath { path: request_path });
    }

    // `path_clean` is purely lexical, so the check above is blind to symlinks:
    // a symlink inside the file transfer directory would otherwise let a request
    // read, overwrite or delete a file outside of it.
    if !is_within_permitted_roots(&clean_path, &file_transfer_dir, &data_dir) {
        return Err(HttpRequestError::InvalidPath { path: request_path });
    }

    Ok(FileTransferPath {
        full: clean_path,
        request: request_path,
        data_dir,
    })
}

/// Check that `path`, once symlinks are resolved, stays inside a directory the
/// File Transfer Service is allowed to serve
///
/// Two roots are permitted:
///
/// - the file transfer directory itself, and
/// - the file cache directory, because the agent deliberately symlinks from the
///   former into the latter so that a downloaded config file can be served over
///   this API (see `create_symlink_for_config_update`).
fn is_within_permitted_roots(
    path: &Utf8Path,
    file_transfer_dir: &ManagedDir,
    data_dir: &DataDir,
) -> bool {
    let Some(resolved) = resolve_symlinks(path) else {
        return false;
    };

    // The roots are resolved the same way as the path itself: canonicalising only one
    // side would make every comparison fail wherever the data directory sits behind a
    // symlink, or has yet to be created.
    [file_transfer_dir.path(), data_dir.cache_dir().path()]
        .into_iter()
        .filter_map(resolve_symlinks)
        .any(|root| resolved.starts_with(root))
}

/// Resolve `path` against the filesystem, tolerating components that don't exist yet
///
/// An upload creates the file, and any missing parent directories, so the path
/// being checked is frequently not present on disk. The deepest existing ancestor
/// is resolved with [Utf8Path::canonicalize_utf8] and the remaining components are
/// appended unchanged — they are safe to append lexically because `path` has
/// already been through [path_clean::clean] and so holds no `.` or `..` component.
///
/// Returns [None] if no ancestor can be resolved at all, or if a component exists
/// but cannot be resolved.
fn resolve_symlinks(path: &Utf8Path) -> Option<Utf8PathBuf> {
    let mut not_yet_created = Vec::new();
    let mut ancestor = path;

    loop {
        if let Ok(mut resolved) = ancestor.canonicalize_utf8() {
            resolved.extend(not_yet_created.iter().rev());
            return Some(resolved);
        }

        // A component that exists on disk yet cannot be canonicalised is a symlink
        // that doesn't resolve: dangling, or a loop. It must not be mistaken for a
        // component waiting to be created, because appending it lexically would hide
        // where it actually points while `File::create` would still follow it and
        // write outside the directory.
        if ancestor.symlink_metadata().is_ok() {
            return None;
        }

        not_yet_created.push(ancestor.file_name()?);
        ancestor = ancestor.parent()?;
    }
}

fn clean_utf8_path(path: &Utf8Path) -> Utf8PathBuf {
    // unwrap is safe because clean returns an utf8 path when given an utf8 path
    Utf8PathBuf::try_from(path_clean::clean(path.as_std_path())).unwrap()
}
