use std::ffi::OsString;
use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;

use async_tempfile::TempFile;
use tokio::io::AsyncWrite;
use tokio::io::AsyncWriteExt;
use tokio::io::BufWriter;

#[derive(thiserror::Error, Debug)]
pub enum PathsError {
    #[error("Directory Error. Check permissions for {1}.")]
    DirCreationFailed(#[source] std::io::Error, PathBuf),

    #[error("File Error. Check permissions for {1}.")]
    FileCreationFailed(#[source] std::io::Error, PathBuf),

    #[error("User's Home Directory not found.")]
    HomeDirNotFound,

    #[error(transparent)]
    IoError(#[from] std::io::Error),

    #[error("Path conversion to String failed: {path:?}.")]
    PathToStringFailed { path: OsString },

    #[error("Couldn't write configuration file, check permissions.")]
    PersistError(#[from] async_tempfile::Error),

    #[error("Directory: {path:?} not found")]
    DirNotFound { path: OsString },

    #[error("Parent directory for the path: {path:?} not found")]
    ParentDirNotFound { path: OsString },

    #[error("Relative path: {path:?} is not permitted. Provide an absolute path instead.")]
    RelativePathNotPermitted { path: OsString },
}

pub fn create_directories(dir_path: impl AsRef<Path>) -> Result<(), PathsError> {
    let dir_path = dir_path.as_ref();
    std::fs::create_dir_all(dir_path)
        .map_err(|error| PathsError::DirCreationFailed(error, dir_path.into()))
}

pub async fn persist_tempfile(
    mut file: BufWriter<TempFile>,
    path_to: impl AsRef<Path>,
) -> Result<(), PathsError> {
    file.flush().await?;
    file.get_ref().sync_all().await?;
    tokio::fs::rename(file.get_ref().file_path(), &path_to)
        .await
        .map_err(|error| PathsError::FileCreationFailed(error, path_to.as_ref().into()))?;

    Ok(())
}

pub fn ok_if_not_found(err: std::io::Error) -> std::io::Result<()> {
    match err.kind() {
        std::io::ErrorKind::NotFound => Ok(()),
        _ => Err(err),
    }
}

/// A DraftFile is a temporary file
/// that can be populated using the `Write` trait
/// then finally and atomically persisted to a target file
/// with permission set to given mode if provided
#[pin_project::pin_project]
pub struct DraftFile {
    #[pin]
    file: BufWriter<TempFile>,
    target: PathBuf,
    mode: Option<u32>,
}

impl DraftFile {
    /// Create a draft for a file
    pub async fn new(target: impl AsRef<Path>) -> Result<DraftFile, PathsError> {
        let target = target.as_ref();

        // Since the persist method will rename the temp file into the target,
        // one has to create the temp file in the same file system as the target.
        let dir = target
            .parent()
            .ok_or_else(|| PathsError::ParentDirNotFound {
                path: target.as_os_str().into(),
            })?;
        let file = BufWriter::new(TempFile::new_in(dir).await?);

        let target = target.to_path_buf();

        Ok(DraftFile {
            file,
            target,
            mode: None,
        })
    }

    /// Provide mode that will be applied to target file after persist operation
    pub fn with_mode(self, mode: u32) -> Self {
        Self {
            mode: Some(mode),
            ..self
        }
    }

    /// Atomically persist the file into its target path and apply permission if provided
    pub async fn persist(self) -> Result<(), PathsError> {
        let target = &self.target;
        persist_tempfile(self.file, target).await?;

        if let Some(mode) = self.mode {
            let perm = Permissions::from_mode(mode);
            tokio::fs::set_permissions(&target, perm).await?;
        }

        Ok(())
    }
}

impl AsyncWrite for DraftFile {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        let this = self.project();
        this.file.poll_write(cx, buf)
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let this = self.project();
        this.file.poll_flush(cx)
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        let this = self.project();
        this.file.poll_shutdown(cx)
    }
}
