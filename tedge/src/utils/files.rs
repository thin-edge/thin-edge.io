use std::path::{Path, PathBuf};
use which::which;

use super::UtilsError;

pub fn build_path_from_home<T: AsRef<Path>>(paths: &[T]) -> Result<String, UtilsError> {
    build_path_from_home_as_path(paths).and_then(pathbuf_to_string)
}

pub fn pathbuf_to_string(pathbuf: PathBuf) -> Result<String, UtilsError> {
    pathbuf
        .into_os_string()
        .into_string()
        .map_err(|_os_string| UtilsError::BridgeConnectionFailed)
}

pub fn sudo_path() -> Result<PathBuf, UtilsError> {
    Ok(which("sudo")?)
}

fn build_path_from_home_as_path<T: AsRef<Path>>(paths: &[T]) -> Result<PathBuf, UtilsError> {
    let home_dir = home_dir().ok_or(UtilsError::ConfigurationExists)?;

    let mut final_path: PathBuf = PathBuf::from(home_dir);
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
    return std::env::var_os("HOME")
        .and_then(|home| if home.is_empty() { None } else { Some(home) })
        .map(PathBuf::from);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[ignore]
    #[test]
    fn build_path_from_home_multiple_arguments() {
        let expected: &str = "/home/test/test/.test";
        std::env::set_var("HOME", "/home/test/");
        assert_eq!(build_path_from_home(&["test", ".test"]).unwrap(), expected);
    }

    #[ignore]
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
}
