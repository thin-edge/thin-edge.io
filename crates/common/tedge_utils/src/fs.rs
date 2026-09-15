use crate::file;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use std::fs as std_fs;
use std::io::Read;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use tokio::fs as tokio_fs;
use tokio::io::AsyncWriteExt;

#[derive(Debug, thiserror::Error)]
pub enum AtomFileError {
    #[error("Writing the content to the file {file:?} failed: {context:?}. source={source:?}")]
    WriteError {
        file: Utf8PathBuf,
        context: String,
        source: std::io::Error,
    },

    #[error(transparent)]
    FromFileError(#[from] file::FileError),
}

pub trait ErrContext<T> {
    fn with_context(
        self,
        context: impl Fn() -> String,
        file: &Utf8Path,
    ) -> Result<T, AtomFileError>;
}

impl<T, E: Into<std::io::Error>> ErrContext<T> for Result<T, E> {
    fn with_context(
        self,
        context: impl Fn() -> String,
        file: &Utf8Path,
    ) -> Result<T, AtomFileError> {
        self.map_err(|err| AtomFileError::WriteError {
            file: file.to_owned(),
            context: context(),
            source: err.into(),
        })
    }
}

/// Write file to filesystem atomically using std::fs synchronously.
///
/// Resulting destination file will have file mode 644. If a file already exists under the
/// destination path, its ownership and mode will be overwritten.
pub fn atomically_write_file_sync(
    dest: impl AsRef<Utf8Path>,
    mut reader: impl Read,
) -> Result<(), AtomFileError> {
    let dest = dest.as_ref();
    // resolve path (including symlinks)
    // if the symlink doesn't exist, (attempt to) create the file it points to
    let dest = match std::fs::read_link(dest) {
        Ok(target) => Utf8PathBuf::try_from(target).unwrap_or_else(|_| dest.to_owned()),
        Err(_) => dest.to_owned(),
    };
    let dest_dir = parent_dir(&dest);

    // removed on drop
    let mut file = tempfile::Builder::new()
        .rand_bytes(6)
        .permissions(std_fs::Permissions::from_mode(0o644))
        .tempfile_in(&dest_dir)
        .with_context(|| "could not create temporary file".to_string(), &dest_dir)?;

    std::io::copy(&mut reader, &mut file).with_context(
        || {
            format!(
                "could not copy the content to the temporary file {:?}",
                file.path(),
            )
        },
        &dest,
    )?;

    // Ensure the content reach the disk
    file.flush().with_context(
        || {
            format!(
                "could not flush the content of the temporary file {:?}",
                file.path(),
            )
        },
        &dest,
    )?;

    file.as_file().sync_all().with_context(
        || {
            format!(
                "could not save the temporary file {:?} to disk",
                file.path(),
            )
        },
        &dest,
    )?;

    // Move the temp file to its destination
    file.persist(&dest)
        .with_context(|| "could not write to destination file".to_string(), &dest)?;

    // Ensure the new name reach the disk
    let dir = std::fs::File::open(&dest_dir)
        .with_context(|| "could not open the directory".to_string(), &dest)?;

    dir.sync_all()
        .with_context(|| "could not save the file to disk".to_string(), &dest)?;

    Ok(())
}

/// Write file to filesystem atomically using tokio::fs asynchronously.
///
/// Resulting destination file will have file mode 644. If a file already exists under the
/// destination path, its ownership and mode will be overwritten.
pub async fn atomically_write_file_async(
    dest: impl AsRef<Utf8Path>,
    content: &[u8],
) -> Result<(), AtomFileError> {
    let dest = dest.as_ref();
    // resolve path (including symlinks)
    // if the symlink doesn't exist, (attempt to) create the file it points to
    let dest = match tokio::fs::read_link(dest).await {
        Ok(target) => Utf8PathBuf::try_from(target).unwrap_or_else(|_| dest.to_owned()),
        Err(_) => dest.to_owned(),
    };
    let dest_dir = parent_dir(&dest);

    // removed on drop if not persisted
    let mut file = tempfile::Builder::new()
        .rand_bytes(6)
        .permissions(std_fs::Permissions::from_mode(0o644))
        .make_in(&dest_dir, |path| {
            let file = std_fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path);
            file.map(tokio_fs::File::from_std)
        })
        .with_context(|| "could not create the temporary file".to_string(), &dest)?;

    file.as_file_mut().write_all(content).await.with_context(
        || format!("could not write the content to the temporary file {file:?}"),
        &dest,
    )?;

    // Ensure the content reach the disk
    file.as_file_mut().flush().await.with_context(
        || format!("could not flush the content of the temporary file {file:?}"),
        &dest,
    )?;

    file.as_file().sync_all().await.with_context(
        || format!("could not save the temporary file {file:?} to disk"),
        &dest,
    )?;

    // Move the temp file to its destination
    file.persist(&dest)
        .with_context(|| "could not create destination file".to_string(), &dest)?;

    // Ensure the new name reach the disk
    let dir = tokio_fs::File::open(&dest_dir)
        .await
        .with_context(|| "could not open the directory".to_string(), &dest)?;

    dir.sync_all()
        .await
        .with_context(|| "could not save the file to disk".to_string(), &dest)?;

    Ok(())
}

fn parent_dir(file: &Utf8Path) -> Utf8PathBuf {
    match file.parent() {
        None => Utf8PathBuf::from("/"),
        Some(path) if path.as_str().is_empty() => Utf8PathBuf::from("."),
        Some(dir) => dir.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use crate::fs::atomically_write_file_async;
    use crate::fs::atomically_write_file_sync;

    use camino::Utf8Path;
    use tempfile::tempdir;

    fn tempdir_utf8_path(dir: &tempfile::TempDir) -> &Utf8Path {
        Utf8Path::from_path(dir.path()).expect("tempdir path is valid UTF-8")
    }

    #[tokio::test]
    async fn atomically_write_file_file_async() {
        let temp_dir = tempdir().unwrap();
        let root = tempdir_utf8_path(&temp_dir);
        let temp_path = root.join("test1");
        let destination_path = root.join("test2");

        let content = "test_data";

        atomically_write_file_async(&destination_path, content.as_bytes())
            .await
            .unwrap();

        std::fs::File::open(&temp_path).unwrap_err();
        let destination_content = std::fs::read(&destination_path).unwrap();
        assert_eq!(destination_content, content.as_bytes());
    }

    #[tokio::test]
    async fn atomically_write_file_file_async_with_symlink() {
        let temp_dir = tempdir().unwrap();
        let root = tempdir_utf8_path(&temp_dir);
        let link_path = root.join("test-link");
        let destination_path = root.join("test-orig");
        let _ = std::fs::write(&destination_path, "dummy contents");
        let _ = std::os::unix::fs::symlink(&destination_path, &link_path);

        let content = "test_data";

        atomically_write_file_async(&destination_path, content.as_bytes())
            .await
            .unwrap();

        let destination_content = std::fs::read(&destination_path).unwrap();
        assert_eq!(destination_content, content.as_bytes());
    }

    #[tokio::test]
    async fn atomically_write_file_file_async_with_broken_symlink() {
        let temp_dir = tempdir().unwrap();
        let root = tempdir_utf8_path(&temp_dir);
        let link_path = root.join("test-link");
        let destination_path = root.join("test-orig");
        let _ = std::os::unix::fs::symlink(&destination_path, &link_path);

        let content = "test_data";

        atomically_write_file_async(&destination_path, content.as_bytes())
            .await
            .unwrap();

        let destination_content = std::fs::read(&destination_path).unwrap();
        assert_eq!(destination_content, content.as_bytes());
    }

    #[test]
    fn atomically_write_file_file_sync() {
        let temp_dir = tempdir().unwrap();
        let root = tempdir_utf8_path(&temp_dir);
        let destination_path = root.join("test2");

        let content = "test_data";

        let () = atomically_write_file_sync(&destination_path, content.as_bytes()).unwrap();

        let destination_content = std::fs::read(&destination_path).unwrap();
        assert_eq!(destination_content, content.as_bytes());
    }

    #[test]
    fn atomically_write_file_file_sync_with_symlink() {
        let temp_dir = tempdir().unwrap();
        let root = tempdir_utf8_path(&temp_dir);
        let link_path = root.join("test-link");
        let destination_path = root.join("test-orig");
        let _ = std::fs::write(&destination_path, "dummy contents");
        let _ = std::os::unix::fs::symlink(&destination_path, &link_path);

        let content = "test_data";

        let () = atomically_write_file_sync(&link_path, content.as_bytes()).unwrap();

        let destination_content = std::fs::read(&destination_path).unwrap();
        assert_eq!(destination_content, content.as_bytes());
    }

    #[test]
    fn atomically_write_file_file_sync_with_broken_symlink() {
        let temp_dir = tempdir().unwrap();
        let root = tempdir_utf8_path(&temp_dir);
        let link_path = root.join("test-link");
        let destination_path = root.join("test-orig");
        let _ = std::os::unix::fs::symlink(&destination_path, &link_path);

        let content = "test_data";

        let () = atomically_write_file_sync(&link_path, content.as_bytes()).unwrap();

        let destination_content = std::fs::read(&destination_path).unwrap();
        assert_eq!(destination_content, content.as_bytes());
    }
}
