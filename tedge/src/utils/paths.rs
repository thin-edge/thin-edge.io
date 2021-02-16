use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use tempfile::{NamedTempFile, PersistError};

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
}

pub fn build_path_from_home<T: AsRef<Path>>(paths: &[T]) -> Result<String, PathsError> {
    build_path_from_home_as_path(paths).and_then(pathbuf_to_string)
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

fn build_path_from_home_as_path<T: AsRef<Path>>(paths: &[T]) -> Result<PathBuf, PathsError> {
    let home_dir = home_dir().ok_or(PathsError::HomeDirNotFound)?;

    let mut final_path: PathBuf = home_dir;
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
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .and_then(|home| if home.is_empty() { None } else { Some(home) })
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[ignore = "Depends on HOME directory"]
    #[test]
    fn build_path_from_home_multiple_arguments() {
        let expected: &str = "/home/test/test/.test";
        std::env::set_var("HOME", "/home/test/");
        assert_eq!(build_path_from_home(&["test", ".test"]).unwrap(), expected);
    }

    #[ignore = "Depends on HOME directory"]
    #[test]
    fn home_dir_test() {
        let home = std::env::var("HOME").unwrap();
        std::env::set_var("HOME", "/home/test/");
        let expected_path = std::path::PathBuf::from("/home/test/");
        assert_eq!(home_dir(), Some(expected_path));

        std::env::remove_var("HOME");
        assert_eq!(home_dir(), None);
        std::env::set_var("HOME", home);
    }

    #[test]
    fn pathbuf_to_string_ok() {
        let pathbuf: PathBuf = "test".into();
        let expected: String = "test".into();
        let result = pathbuf_to_string(pathbuf).unwrap();
        assert_eq!(result, expected);
    }
}
