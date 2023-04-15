use std::fs as std_fs;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

use tokio::fs as tokio_fs;
use tokio::io::AsyncWriteExt;

/// Write file to filesystem atomically using std::fs synchronously.
pub fn atomically_write_file_sync(dest: impl AsRef<Path>, content: &[u8]) -> std::io::Result<()> {
    let mut tempfile = PathBuf::from(dest.as_ref());
    tempfile.set_extension("tmp");

    let mut file = std_fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tempfile)?;

    if let Err(err) = file.write_all(content) {
        let _ = std_fs::remove_file(tempfile);
        return Err(err);
    }

    file.flush()?;

    if let Err(err) = std_fs::rename(&tempfile, dest) {
        let _ = std_fs::remove_file(tempfile);
        return Err(err);
    }

    Ok(())
}

/// Write file to filesystem atomically using tokio::fs asynchronously.
pub async fn atomically_write_file_async(
    dest: impl AsRef<Path>,
    content: &[u8],
) -> std::io::Result<()> {
    let mut tempfile = PathBuf::from(dest.as_ref());
    tempfile.set_extension("tmp");

    let mut file = tokio_fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tempfile)
        .await?;

    if let Err(err) = file.write_all(content).await {
        tokio_fs::remove_file(tempfile).await?;
        return Err(err);
    }

    file.flush().await?;

    if let Err(err) = tokio_fs::rename(&tempfile, dest).await {
        tokio_fs::remove_file(tempfile).await?;
        return Err(err);
    }

    Ok(())
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
