use std::{
    ffi::OsString,
    fs::File,
    path::{Path, PathBuf},
    str::FromStr,
};

use tempfile::{NamedTempFile, PersistError};

use super::users::UserManager;

const ETC_PATH: &str = "/etc";
pub const TEDGE_ETC_DIR: &str = "tedge";
pub const TEDGE_HOME_DIR: &str = ".tedge";

#[derive(thiserror::Error, Debug)]
pub enum PathsError {
    #[error("Directory Error. Check permissions for {1}.")]
    DirCreationFailed(#[source] std::io::Error, String),

    #[error("File Error. Check permissions for {1}.")]
    FileCreationFailed(#[source] PersistError, String),

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

pub fn build_path_for_sudo_or_user<T: AsRef<Path>>(paths: &[T]) -> Result<String, PathsError> {
    build_path_for_sudo_or_user_as_path(paths).and_then(pathbuf_to_string)
}

pub fn pathbuf_to_string(pathbuf: PathBuf) -> Result<String, PathsError> {
    pathbuf
        .into_os_string()
        .into_string()
        .map_err(|os_string| PathsError::PathToStringFailed { path: os_string })
}

pub fn create_directories(dir_path: &str) -> Result<(), PathsError> {
    std::fs::create_dir_all(&dir_path)
        .map_err(|error| PathsError::DirCreationFailed(error, dir_path.into()))
}

pub fn persist_tempfile(file: NamedTempFile, path_to: &str) -> Result<(), PathsError> {
    let _ = file
        .persist(&path_to)
        .map_err(|error| PathsError::FileCreationFailed(error, path_to.into()))?;

    Ok(())
}

pub fn build_path_for_sudo_or_user_as_path<T: AsRef<Path>>(
    paths: &[T],
) -> Result<PathBuf, PathsError> {
    let mut final_path: PathBuf = if UserManager::running_as_root() {
        PathBuf::from_str(ETC_PATH).unwrap().join(TEDGE_ETC_DIR)
    } else {
        home_dir()
            .ok_or(PathsError::HomeDirNotFound)?
            .join(TEDGE_HOME_DIR)
    };

    for path in paths {
        final_path.push(path);
    }

    Ok(final_path)
}

// This isn't complete way to retrieve HOME dir from the user.
// We could parse passwd file to get actual home path if we can get user name.
// I suppose rust provides some way to do it or allows through c bindings... But this implies unsafe code.
// Another alternative is to use deprecated env::home_dir() -1
// https://github.com/rust-lang/rust/issues/71684
pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .and_then(|home| if home.is_empty() { None } else { Some(home) })
        .map(PathBuf::from)
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

pub fn validate_parent_dir_exists(path: impl AsRef<Path>) -> Result<(), PathsError> {
    let path = path.as_ref();
    if path.is_relative() {
        Err(PathsError::RelativePathNotPermitted { path: path.into() })
    } else {
        match path.parent() {
            None => Err(PathsError::ParentDirNotFound { path: path.into() }),
            Some(parent) => {
                if !parent.exists() {
                    Err(PathsError::DirNotFound {
                        path: parent.into(),
                    })
                } else {
                    Ok(())
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;

    #[test]
    fn pathbuf_to_string_ok() {
        let pathbuf: PathBuf = "test".into();
        let expected: String = "test".into();
        let result = pathbuf_to_string(pathbuf).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    #[cfg(unix)] // On windows the error is unexpectedly RelativePathNotPermitted
    fn validate_path_non_existent() {
        let result = validate_parent_dir_exists(Path::new("/non/existent/path"));
        assert_matches!(result.unwrap_err(), PathsError::DirNotFound { .. });
    }

    #[test]
    #[cfg(unix)] // On windows the error is unexpectedly RelativePathNotPermitted
    fn validate_parent_dir_non_existent() {
        let result = validate_parent_dir_exists(Path::new("/"));
        assert_matches!(result.unwrap_err(), PathsError::ParentDirNotFound { .. });
    }

    #[test]
    fn validate_parent_dir_relative_path() {
        let result = validate_parent_dir_exists(Path::new("test.txt"));
        assert_matches!(
            result.unwrap_err(),
            PathsError::RelativePathNotPermitted { .. }
        );
    }
}
