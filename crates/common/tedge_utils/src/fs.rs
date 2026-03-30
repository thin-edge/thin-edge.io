use crate::file;
use std::fs as std_fs;
use std::io::Read;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
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

    #[error(transparent)]
    FromFileError(#[from] file::FileError),
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

/// Write file to filesystem atomically using std::fs synchronously.
///
/// Resulting destination file will have file mode 644. If a file already exists under the
/// destination path, its ownership and mode will be overwritten.
pub fn atomically_write_file_sync(
    dest: impl AsRef<Path>,
    mut reader: impl Read,
) -> Result<(), AtomFileError> {
    let dest = dest.as_ref();
    // resolve path (including symlinks)
    // if the symlink doesn't exist, (attempt to) create the file it points to
    let dest = std::fs::read_link(dest).unwrap_or_else(|_| dest.to_path_buf());
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
    let dir = std::fs::File::open(dest_dir)
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
    dest: impl AsRef<Path>,
    content: &[u8],
) -> Result<(), AtomFileError> {
    let dest = dest.as_ref();
    // resolve path (including symlinks)
    // if the symlink doesn't exist, (attempt to) create the file it points to
    let dest = tokio::fs::read_link(dest)
        .await
        .unwrap_or_else(|_| dest.to_path_buf());
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
        || format!("could not write the content to the temporary file {file:?}",),
        &dest,
    )?;

    // Ensure the content reach the disk
    file.as_file_mut().flush().await.with_context(
        || format!("could not flush the content of the temporary file {file:?}",),
        &dest,
    )?;

    file.as_file().sync_all().await.with_context(
        || format!("could not save the temporary file {file:?} to disk",),
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

fn parent_dir(file: &Path) -> PathBuf {
    match file.parent() {
        None => Path::new("/").into(),
        Some(path) if nix::NixPath::is_empty(path) => Path::new(".").into(),
        Some(dir) => dir.into(),
    }
}

/// Persists a file using the template pattern:
///
/// - `{name}` - active file (updated only if not overridden or disabled)
/// - `{name}.template` - canonical template (always updated)
/// - `{name}.disabled` - marker file indicating the file is disabled
///
/// If a user has modified the `name` file (making it differ from the `name.template`),
/// or if a `.disabled` marker exists, the active file will not be updated.
/// However, the template will always be refreshed with the latest definition.
pub async fn persist_file_with_template(
    dir: impl AsRef<Path>,
    name: &str,
    content: &str,
) -> Result<(), AtomFileError> {
    let dir = dir.as_ref();
    let config_path = dir.join(name);
    let template_path = dir.join(format!("{}.template", name));
    let disabled_path = dir.join(format!("{}.disabled", name));

    // Detect if user has customized the config by comparing with template
    let prior_config: Option<Vec<u8>> = tokio_fs::read(&config_path).await.ok();
    let prior_template: Option<Vec<u8>> = tokio_fs::read(&template_path).await.ok();
    let overridden = prior_config != prior_template;
    let disabled = tokio_fs::try_exists(&disabled_path).await.unwrap_or(false);

    file::create_directory_with_defaults(dir).await?;

    // Update the active config only if it hasn't been customized or disabled
    if !overridden && !disabled {
        atomically_write_file_async(&config_path, content.as_bytes()).await?;
    }

    // Always update the template file with the latest definition
    atomically_write_file_async(&template_path, content.as_bytes()).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::fs::atomically_write_file_async;
    use crate::fs::atomically_write_file_sync;
    use crate::fs::persist_file_with_template;
    use crate::fs::AtomFileError;

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

    #[tokio::test]
    async fn atomically_write_file_file_async_with_symlink() {
        let temp_dir = tempdir().unwrap();
        let link_path = temp_dir.path().join("test-link");
        let destination_path = temp_dir.path().join("test-orig");
        let _ = std::fs::write(destination_path.clone(), "dummy contents");
        let _ = std::os::unix::fs::symlink(destination_path.clone(), link_path.clone());

        let content = "test_data";

        atomically_write_file_async(destination_path.clone(), content.as_bytes())
            .await
            .unwrap();

        if let Ok(destination_content) = std::fs::read(destination_path) {
            assert_eq!(destination_content, content.as_bytes());
        } else {
            panic!("failed to read the new file");
        }
    }

    #[tokio::test]
    async fn atomically_write_file_file_async_with_broken_symlink() {
        let temp_dir = tempdir().unwrap();
        let link_path = temp_dir.path().join("test-link");
        let destination_path = temp_dir.path().join("test-orig");
        let _ = std::os::unix::fs::symlink(destination_path.clone(), link_path.clone());

        let content = "test_data";

        atomically_write_file_async(destination_path.clone(), content.as_bytes())
            .await
            .unwrap();

        if let Ok(destination_content) = std::fs::read(destination_path) {
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

    #[test]
    fn atomically_write_file_file_sync_with_symlink() {
        let temp_dir = tempdir().unwrap();
        let link_path = temp_dir.path().join("test-link");
        let destination_path = temp_dir.path().join("test-orig");
        let _ = std::fs::write(destination_path.clone(), "dummy contents");
        let _ = std::os::unix::fs::symlink(destination_path.clone(), link_path.clone());

        let content = "test_data";

        let () = atomically_write_file_sync(link_path.clone(), content.as_bytes()).unwrap();

        if let Ok(destination_content) = std::fs::read(destination_path) {
            assert_eq!(destination_content, content.as_bytes());
        } else {
            panic!("failed to read the new file");
        }
    }

    #[test]
    fn atomically_write_file_file_sync_with_broken_symlink() {
        let temp_dir = tempdir().unwrap();
        let link_path = temp_dir.path().join("test-link");
        let destination_path = temp_dir.path().join("test-orig");
        let _ = std::os::unix::fs::symlink(destination_path.clone(), link_path.clone());

        let content = "test_data";

        let () = atomically_write_file_sync(link_path.clone(), content.as_bytes()).unwrap();

        if let Ok(destination_content) = std::fs::read(destination_path) {
            assert_eq!(destination_content, content.as_bytes());
        } else {
            panic!("failed to read the new file");
        }
    }

    #[tokio::test]
    async fn persist_file_with_template_creates_both_files_when_fresh() {
        let temp_dir = tempdir().unwrap();
        let content = "test config content";

        let _: Result<(), AtomFileError> =
            persist_file_with_template(temp_dir.path(), "test.toml", content).await;

        let config_file = temp_dir.path().join("test.toml");
        let template_file = temp_dir.path().join("test.toml.template");

        assert!(config_file.exists(), "config file should exist");
        assert!(template_file.exists(), "template file should exist");

        let config_content = std::fs::read_to_string(&config_file).unwrap();
        let template_content = std::fs::read_to_string(&template_file).unwrap();

        assert_eq!(config_content, content, "config content should match");
        assert_eq!(template_content, content, "template content should match");
    }

    #[tokio::test]
    async fn persist_file_with_template_preserves_overriden_file() {
        let temp_dir = tempdir().unwrap();
        let original_content = "original config";
        let custom_content = "custom user config";

        // First call - creates both files
        let _: Result<(), AtomFileError> =
            persist_file_with_template(temp_dir.path(), "test.toml", original_content).await;

        // User customizes the config file
        let config_file = temp_dir.path().join("test.toml");
        tokio::fs::write(&config_file, custom_content)
            .await
            .unwrap();

        let new_content = "new content";
        // Second call - config is customized, should only update template
        let _: Result<(), AtomFileError> =
            persist_file_with_template(temp_dir.path(), "test.toml", new_content).await;

        // Config file should remain unchanged (user customization preserved)
        let config_content = std::fs::read_to_string(&config_file).unwrap();
        assert_eq!(
            config_content, custom_content,
            "config file should preserve user customization"
        );

        // Template file should be updated
        let template_file = temp_dir.path().join("test.toml.template");
        let template_content = std::fs::read_to_string(&template_file).unwrap();
        assert_eq!(
            template_content, new_content,
            "template file should be updated"
        );
    }

    #[tokio::test]
    async fn persist_file_with_template_updates_both_when_unchanged() {
        let temp_dir = tempdir().unwrap();
        let original_content = "original config";

        // First call - creates both files
        let _: Result<(), AtomFileError> =
            persist_file_with_template(temp_dir.path(), "test.toml", original_content).await;

        let new_content = "updated config";
        // Second call - config matches template, should update both
        let _: Result<(), AtomFileError> =
            persist_file_with_template(temp_dir.path(), "test.toml", new_content).await;

        let config_file = temp_dir.path().join("test.toml");
        let template_file = temp_dir.path().join("test.toml.template");

        let config_content = std::fs::read_to_string(&config_file).unwrap();
        let template_content = std::fs::read_to_string(&template_file).unwrap();

        assert_eq!(config_content, new_content, "config file should be updated");
        assert_eq!(
            template_content, new_content,
            "template file should be updated"
        );
    }

    #[tokio::test]
    async fn persist_file_with_template_preserves_original_if_disabled() {
        let temp_dir = tempdir().unwrap();
        let original_content = "original config";

        // First call - creates both the config and the template files
        let _ = persist_file_with_template(temp_dir.path(), "test.toml", original_content).await;

        // Create disabled marker
        tokio::fs::write(temp_dir.path().join("test.toml.disabled"), "")
            .await
            .unwrap();

        let new_content = "new content";
        // Second call - disabled marker exists, should only update template
        let _ = persist_file_with_template(temp_dir.path(), "test.toml", new_content).await;

        // Config file should remain unchanged because disabled
        let config_file = temp_dir.path().join("test.toml");
        let config_content = tokio::fs::read_to_string(&config_file).await.unwrap();
        assert_eq!(
            config_content, original_content,
            "config file should remain unchanged when disabled"
        );

        // Template file should be updated
        let template_file = temp_dir.path().join("test.toml.template");
        let template_content = std::fs::read_to_string(&template_file).unwrap();
        assert_eq!(
            template_content, new_content,
            "template file should be updated even when disabled"
        );
    }
}
