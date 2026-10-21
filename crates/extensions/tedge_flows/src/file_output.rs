use camino::Utf8Path;
use camino::Utf8PathBuf;
use std::io::ErrorKind;

#[derive(thiserror::Error, Debug)]
pub enum OutputFileError {
    #[error("invalid file name {name:?}: {reason}")]
    InvalidName { name: String, reason: &'static str },

    #[error("cannot write to {path}: {reason}")]
    UnsafePath {
        path: Utf8PathBuf,
        reason: &'static str,
    },

    #[error("cannot create directory {path}: {error}")]
    CannotCreateDirectory {
        path: Utf8PathBuf,
        error: std::io::Error,
    },

    #[error("cannot access {path}: {error}")]
    CannotAccess {
        path: Utf8PathBuf,
        error: std::io::Error,
    },
}

/// Check that a file name provided by a message is a relative path which cannot escape
/// the output directory, returning the segments of that path
pub fn file_name_segments(name: &str) -> Result<Vec<&str>, OutputFileError> {
    let invalid = |reason| OutputFileError::InvalidName {
        name: name.to_string(),
        reason,
    };
    if name.is_empty() {
        return Err(invalid("the name is empty"));
    }
    if name.starts_with('/') {
        return Err(invalid("absolute paths are not allowed"));
    }
    if name.chars().any(|c| c == '\\' || c.is_control()) {
        return Err(invalid("backslash and control characters are not allowed"));
    }
    let segments: Vec<&str> = name.split('/').collect();
    if segments
        .iter()
        .any(|segment| segment.is_empty() || *segment == "." || *segment == "..")
    {
        return Err(invalid("empty, '.' and '..' path segments are not allowed"));
    }
    Ok(segments)
}

/// Return the path of the file named by a message within an output directory,
/// creating the sub-directories of that path if needed.
///
/// The output directory is defined by the flow definition and is trusted.
/// However, symbolic links below that directory are rejected,
/// so a message cannot be written outside of the directory.
pub async fn prepare_output_file(
    dir: &Utf8Path,
    name: &str,
) -> Result<Utf8PathBuf, OutputFileError> {
    let segments = file_name_segments(name)?;
    let Some((file_name, parents)) = segments.split_last() else {
        unreachable!("splitting a string returns at least one segment")
    };

    let mut path = dir.to_path_buf();
    for segment in parents {
        path.push(segment);
        match tokio::fs::symlink_metadata(&path).await {
            Ok(metadata) if metadata.is_symlink() => {
                return Err(unsafe_path(path, "symbolic links are not allowed"))
            }
            Ok(metadata) if metadata.is_dir() => {}
            Ok(_) => return Err(unsafe_path(path, "not a directory")),
            Err(err) if err.kind() == ErrorKind::NotFound => {
                if let Err(error) = tokio::fs::create_dir(&path).await {
                    return Err(OutputFileError::CannotCreateDirectory { path, error });
                }
            }
            Err(error) => return Err(OutputFileError::CannotAccess { path, error }),
        }
    }

    path.push(file_name);
    match tokio::fs::symlink_metadata(&path).await {
        Ok(metadata) if metadata.is_symlink() => {
            Err(unsafe_path(path, "symbolic links are not allowed"))
        }
        Ok(metadata) if metadata.is_file() => Ok(path),
        Ok(_) => Err(unsafe_path(path, "not a regular file")),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(path),
        Err(error) => Err(OutputFileError::CannotAccess { path, error }),
    }
}

fn unsafe_path(path: Utf8PathBuf, reason: &'static str) -> OutputFileError {
    OutputFileError::UnsafePath { path, reason }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn utf8_path(dir: &TempDir) -> &Utf8Path {
        Utf8Path::from_path(dir.path()).unwrap()
    }

    #[test]
    fn accepts_relative_file_names() {
        for name in [
            "data.parquet",
            "date=2026-09-14/data.parquet",
            "a/b/c.bin",
            "..hidden",
            "file..name",
        ] {
            assert!(file_name_segments(name).is_ok(), "{name:?} should be valid");
        }
    }

    #[test]
    fn rejects_file_names_escaping_the_directory() {
        for name in [
            "",
            ".",
            "..",
            "/etc/passwd",
            "../escaped",
            "a/../../escaped",
            "a/..",
            "./a",
            "a//b",
            "a/",
            "a\\..\\b",
            "a\0b",
            "a\nfake log line",
        ] {
            assert!(
                matches!(
                    file_name_segments(name),
                    Err(OutputFileError::InvalidName { .. })
                ),
                "{name:?} should be rejected"
            );
        }
    }

    #[tokio::test]
    async fn creates_sub_directories() {
        let dir = tempfile::tempdir().unwrap();
        let root = utf8_path(&dir);

        let path = prepare_output_file(root, "a/b/c.bin").await.unwrap();

        assert_eq!(path, root.join("a/b/c.bin"));
        assert!(root.join("a/b").is_dir());
        assert!(!path.exists(), "the file itself is not created");
    }

    #[tokio::test]
    async fn rejects_a_file_in_place_of_a_directory() {
        let dir = tempfile::tempdir().unwrap();
        let root = utf8_path(&dir);
        std::fs::write(root.join("a"), "").unwrap();

        let result = prepare_output_file(root, "a/b.bin").await;

        assert!(matches!(result, Err(OutputFileError::UnsafePath { .. })));
    }

    #[tokio::test]
    async fn accepts_an_existing_regular_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = utf8_path(&dir);
        std::fs::write(root.join("file.bin"), "").unwrap();

        let path = prepare_output_file(root, "file.bin").await.unwrap();

        assert_eq!(path, root.join("file.bin"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn rejects_special_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = utf8_path(&dir);
        let status = std::process::Command::new("mkfifo")
            .arg(root.join("fifo"))
            .status()
            .unwrap();
        assert!(status.success());

        let result = prepare_output_file(root, "fifo").await;

        assert!(matches!(result, Err(OutputFileError::UnsafePath { .. })));
    }

    #[tokio::test]
    async fn rejects_an_existing_directory_as_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = utf8_path(&dir);
        std::fs::create_dir(root.join("a")).unwrap();

        let result = prepare_output_file(root, "a").await;

        assert!(matches!(result, Err(OutputFileError::UnsafePath { .. })));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn rejects_symbolic_links_to_directories() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let root = utf8_path(&dir);
        std::os::unix::fs::symlink(outside.path(), root.join("link")).unwrap();

        for name in ["link/file.bin", "link/sub/file.bin"] {
            let result = prepare_output_file(root, name).await;
            assert!(
                matches!(result, Err(OutputFileError::UnsafePath { .. })),
                "{name:?} should be rejected"
            );
        }
        assert!(!outside.path().join("sub").exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn rejects_symbolic_links_to_files() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let root = utf8_path(&dir);
        std::os::unix::fs::symlink(outside.path().join("target"), root.join("file.bin")).unwrap();

        let result = prepare_output_file(root, "file.bin").await;

        assert!(matches!(result, Err(OutputFileError::UnsafePath { .. })));
    }
}
