use std::ffi::OsString;
use std::fs::File;
use std::path::Path;
use std::path::PathBuf;

use std::io::Write;
use tempfile::NamedTempFile;
use tempfile::PersistError;

#[derive(thiserror::Error, Debug)]
pub enum PathsError {
    #[error("Directory Error. Check permissions for {1}.")]
    DirCreationFailed(#[source] std::io::Error, PathBuf),

    #[error("File Error. Check permissions for {1}.")]
    FileCreationFailed(#[source] PersistError, PathBuf),

    #[error("User's Home Directory not found.")]
    HomeDirNotFound,

    #[error(transparent)]
    IoError(#[from] std::io::Error),

    #[error("Path conversion to String failed: {path:?}.")]
    PathToStringFailed { path: OsString },

    #[error("Couldn't write configuration file, check permissions.")]
    PersistError(#[from] PersistError),

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

pub fn persist_tempfile(file: NamedTempFile, path_to: impl AsRef<Path>) -> Result<(), PathsError> {
    let path_to = path_to.as_ref();
    let _ = file
        .persist(path_to)
        .map_err(|error| PathsError::FileCreationFailed(error, path_to.into()))?;

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
pub struct DraftFile {
    file: NamedTempFile,
    target: PathBuf,
    mode: Option<u32>,
}

impl DraftFile {
    /// Create a draft for a file
    pub fn new(target: impl AsRef<Path>) -> Result<DraftFile, PathsError> {
        let target = target.as_ref();

        // Since the persist method will rename the temp file into the target,
        // one has to create the temp file in the same file system as the target.
        let dir = target
            .parent()
            .ok_or_else(|| PathsError::ParentDirNotFound {
                path: target.as_os_str().into(),
            })?;
        let file = NamedTempFile::new_in(dir)?;

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
    pub fn persist(self) -> Result<(), PathsError> {
        let target = &self.target;
        let file = self
            .file
            .persist(target)
            .map_err(|error| PathsError::FileCreationFailed(error, target.into()))?;

        if let Some(mode) = self.mode {
            set_permission(&file, mode)?;
        }

        Ok(())
    }
}

impl Write for DraftFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.file.write(buf)
    }
    #[inline]
    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}

/// Set the permission modes of a Unix file.
#[cfg(not(windows))]
pub fn set_permission(file: &File, mode: u32) -> Result<(), std::io::Error> {
    use std::os::unix::fs::PermissionsExt;
    let mut perm = file.metadata()?.permissions();
    perm.set_mode(mode);
    file.set_permissions(perm)
}

/// On windows, no file permission modes are changed.
///
/// So Windows might be used for dev even if not supported.
#[cfg(windows)]
pub fn set_permission(_file: &File, _mode: u32) -> Result<(), std::io::Error> {
    Ok(())
}

pub async fn validate_parent_dir_exists(path: impl AsRef<Path>) -> Result<(), PathsError> {
    let path = path.as_ref();
    if path.is_relative() {
        return Err(PathsError::RelativePathNotPermitted { path: path.into() });
    };
    match path.parent() {
        None => Err(PathsError::ParentDirNotFound { path: path.into() }),
        Some(parent) => match tokio::fs::try_exists(parent).await {
            Ok(true) => Ok(()),
            _ => Err(PathsError::DirNotFound {
                path: parent.into(),
            }),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;

    #[tokio::test]
    #[cfg(unix)] // On windows the error is unexpectedly RelativePathNotPermitted
    async fn validate_path_non_existent() {
        let result = validate_parent_dir_exists(Path::new("/non/existent/path")).await;
        assert_matches!(result.unwrap_err(), PathsError::DirNotFound { .. });
    }

    #[tokio::test]
    #[cfg(unix)] // On windows the error is unexpectedly RelativePathNotPermitted
    async fn validate_parent_dir_non_existent() {
        let result = validate_parent_dir_exists(Path::new("/")).await;
        assert_matches!(result.unwrap_err(), PathsError::ParentDirNotFound { .. });
    }

    #[tokio::test]
    async fn validate_parent_dir_relative_path() {
        let result = validate_parent_dir_exists(Path::new("test.txt")).await;
        assert_matches!(
            result.unwrap_err(),
            PathsError::RelativePathNotPermitted { .. }
        );
    }
}
