use nix::NixPath;
use std::fs as std_fs;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

use tokio::fs as tokio_fs;
use tokio::io::AsyncWriteExt;

#[derive(Debug, thiserror::Error)]
pub enum AtomFileError {
    #[error("Writing the content to the file {file:?} failed: {context:?}. source={source:?}")]
    WriteError {
        file: Box<Path>,
        context: String,
        source: std::io::Error,
    },
}

pub trait ErrContext<T> {
    fn with_context(
        self,
        context: impl Fn() -> String,
        file: impl AsRef<Path>,
    ) -> Result<T, AtomFileError>;
}

impl<T, E: Into<std::io::Error>> ErrContext<T> for Result<T, E> {
    fn with_context(
        self,
        context: impl Fn() -> String,
        file: impl AsRef<Path>,
    ) -> Result<T, AtomFileError> {
        self.map_err(|err| AtomFileError::WriteError {
            file: Box::from(file.as_ref()),
            context: context(),
            source: err.into(),
        })
    }
}

pub struct TempFile(PathBuf);

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std_fs::remove_file(&self.0);
    }
}

/// Write file to filesystem atomically using std::fs synchronously.
pub fn atomically_write_file_sync(
    dest: impl AsRef<Path>,
    mut reader: impl Read,
) -> Result<(), AtomFileError> {
    let dest_dir = parent_dir(dest.as_ref());
    // FIXME: `.with_extension` replaces file extension, so if we used this
    // function to write files `file.txt`, `file.bin`, `file.jpg`, etc.
    // concurrently, then this will result in an error
    let tempfile = TempFile(PathBuf::from(dest.as_ref()).with_extension("tmp"));

    // Write the content on a temp file
    let mut file = std_fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tempfile.0)
        .with_context(
            || format!("could not create the temporary file {:?}", tempfile.0),
            &dest,
        )?;

    std::io::copy(&mut reader, &mut file).with_context(
        || {
            format!(
                "could not copy the content to the temporary file {:?}",
                tempfile.0,
            )
        },
        &dest,
    )?;

    // Ensure the content reach the disk
    file.flush().with_context(
        || {
            format!(
                "could not flush the content of the temporary file {:?}",
                tempfile.0,
            )
        },
        &dest,
    )?;

    file.sync_all().with_context(
        || format!("could not save the temporary file {:?} to disk", tempfile.0,),
        &dest,
    )?;

    // Move the temp file to its destination
    std_fs::rename(&tempfile.0, &dest).with_context(
        || {
            format!(
                "could not move the file from {:?} to {:?}",
                tempfile.0,
                dest.as_ref(),
            )
        },
        &dest,
    )?;

    // Ensure the new name reach the disk
    let dir = std::fs::File::open(dest_dir)
        .with_context(|| "could not open the directory".to_string(), &dest)?;

    dir.sync_all()
        .with_context(|| "could not save the file to disk".to_string(), &dest)?;

    Ok(())
}

/// Write file to filesystem atomically using tokio::fs asynchronously.
pub async fn atomically_write_file_async(
    dest: impl AsRef<Path>,
    content: &[u8],
) -> Result<(), AtomFileError> {
    let dest_dir = parent_dir(dest.as_ref());
    let tempfile = PathBuf::from(dest.as_ref()).with_extension("tmp");

    // Write the content on a temp file
    let mut file = tokio_fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tempfile)
        .await
        .with_context(
            || format!("could not create the temporary file {tempfile:?}"),
            &dest,
        )?;

    if let Err(err) = file.write_all(content).await.with_context(
        || format!("could not write the content to the temporary file {tempfile:?}",),
        &dest,
    ) {
        let _ = tokio_fs::remove_file(&tempfile).await;
        return Err(err);
    }

    // Ensure the content reach the disk
    if let Err(err) = file.flush().await.with_context(
        || format!("could not flush the content of the temporary file {tempfile:?}",),
        &dest,
    ) {
        let _ = tokio_fs::remove_file(&tempfile).await;
        return Err(err);
    }

    if let Err(err) = file.sync_all().await.with_context(
        || format!("could not save the temporary file {tempfile:?} to disk",),
        &dest,
    ) {
        let _ = tokio_fs::remove_file(&tempfile).await;
        return Err(err);
    }

    // Move the temp file to its destination
    if let Err(err) = tokio_fs::rename(&tempfile, &dest).await.with_context(
        || {
            format!(
                "could not move the file from {tempfile:?} to {:?}",
                dest.as_ref()
            )
        },
        &dest,
    ) {
        let _ = tokio_fs::remove_file(&tempfile).await;
        return Err(err);
    }

    // Ensure the new name reach the disk
    let dir = tokio_fs::File::open(&dest_dir)
        .await
        .with_context(|| "could not open the directory".to_string(), &dest)?;

    dir.sync_all()
        .await
        .with_context(|| "could not save the file to disk".to_string(), &dest)?;

    Ok(())
}

fn parent_dir(file: &Path) -> PathBuf {
    match file.parent() {
        None => Path::new("/").into(),
        Some(path) if path.is_empty() => Path::new(".").into(),
        Some(dir) => dir.into(),
    }
}

#[cfg(test)]
mod tests {
    use crate::fs::atomically_write_file_async;
    use crate::fs::atomically_write_file_sync;

    use tempfile::tempdir;

    #[tokio::test]
    async fn atomically_write_file_file_async() {
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path().join("test1");
        let destination_path = temp_dir.path().join("test2");

        let content = "test_data";

        atomically_write_file_async(&destination_path, content.as_bytes())
            .await
            .unwrap();

        std::fs::File::open(&temp_path).unwrap_err();
        if let Ok(destination_content) = std::fs::read(&destination_path) {
            assert_eq!(destination_content, content.as_bytes());
        } else {
            panic!("failed to read the new file");
        }
    }

    #[test]
    fn atomically_write_file_file_sync() {
        let temp_dir = tempdir().unwrap();
        let destination_path = temp_dir.path().join("test2");

        let content = "test_data";

        let () = atomically_write_file_sync(&destination_path, content.as_bytes()).unwrap();

        if let Ok(destination_content) = std::fs::read(&destination_path) {
            assert_eq!(destination_content, content.as_bytes());
        } else {
            panic!("failed to read the new file");
        }
    }
}
