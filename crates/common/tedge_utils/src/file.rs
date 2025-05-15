use futures::TryFutureExt;
use nix::unistd::*;
use std::io::Error;
use std::os::unix::fs::MetadataExt;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;
use tokio::fs;
use tokio::io;
use tokio::io::AsyncWriteExt as _;
use tracing::debug;
use uzers::get_group_by_name;
use uzers::get_user_by_name;

#[derive(thiserror::Error, Debug)]
pub enum FileError {
    #[error("Creating the directory failed: {dir:?}.")]
    DirectoryCreateFailed { dir: String, from: std::io::Error },

    #[error("Creating the file failed: {file:?}.")]
    FileCreateFailed { file: String, from: std::io::Error },

    #[error("Failed to change owner: {name:?}.")]
    MetaDataError { name: String, from: std::io::Error },

    #[error("Failed to change permissions of file: {name:?}.")]
    ChangeModeError { name: String, from: std::io::Error },

    #[error("User not found: {user:?}.")]
    UserNotFound { user: String },

    #[error("Group not found: {group:?}.")]
    GroupNotFound { group: String },

    #[error(transparent)]
    Errno(#[from] nix::errno::Errno),

    #[error("The path is not accessible. {path:?}")]
    PathNotAccessible { path: PathBuf },

    #[error("Writing the content to the file failed: {file:?}.")]
    WriteContentFailed { file: String, from: std::io::Error },

    #[error("Could not save the file {file:?} to disk. Received error: {from:?}.")]
    FailedToSync { file: PathBuf, from: std::io::Error },

    #[error("The path {path:?} is invalid")]
    InvalidFileName {
        path: PathBuf,
        // Can be io::Error or Utf8Error
        source: anyhow::Error,
    },

    #[error("The Path is not valid unicode. {path:?}")]
    InvalidUnicode { path: PathBuf },

    #[error("The Path does not name a valid file. {path:?}")]
    InvalidName { path: PathBuf },

    #[error(transparent)]
    FromIoError(#[from] std::io::Error),

    #[error("No write access to {path:?}")]
    NoWriteAccess { path: PathBuf },

    #[error(transparent)]
    FileMove(#[from] FileMoveError),

    #[error("Failed to create a symlink {link:?}: {source:?}.")]
    CreateSymlinkFailed {
        link: PathBuf,
        source: std::io::Error,
    },
}

pub async fn create_directory<P: AsRef<Path>>(
    dir: P,
    permissions: &PermissionEntry,
) -> Result<(), FileError> {
    permissions.create_directory(dir.as_ref()).await
}
/// Create the directory owned by the user running this API with default directory permissions
pub async fn create_directory_with_defaults<P: AsRef<Path>>(dir: P) -> Result<(), FileError> {
    create_directory(dir, &PermissionEntry::default()).await
}

pub async fn create_directory_with_user_group(
    dir: impl AsRef<Path>,
    user: &str,
    group: &str,
    mode: u32,
) -> Result<(), FileError> {
    let perm_entry = PermissionEntry::new(Some(user.into()), Some(group.into()), Some(mode));
    perm_entry.create_directory(dir.as_ref()).await
}

pub async fn create_file<P: AsRef<Path>>(
    file: P,
    content: Option<&str>,
    permissions: PermissionEntry,
) -> Result<(), FileError> {
    permissions.create_file(file.as_ref(), content).await
}

/// Create the directory owned by the user running this API with default file permissions
pub async fn create_file_with_defaults<P: AsRef<Path>>(
    file: P,
    content: Option<&str>,
) -> Result<(), FileError> {
    create_file(file, content, PermissionEntry::default()).await
}

pub async fn create_file_with_mode(
    file: impl AsRef<Path>,
    content: Option<&str>,
    mode: u32,
) -> Result<(), FileError> {
    let perm_entry = PermissionEntry::new(None, None, Some(mode));
    perm_entry.create_file(file.as_ref(), content).await
}

pub async fn path_exists(path: impl AsRef<Path>) -> bool {
    tokio::fs::try_exists(path).await.unwrap_or(false)
}

pub async fn create_file_with_user_group(
    file: impl AsRef<Path>,
    user: &str,
    group: &str,
    mode: u32,
    default_content: Option<&str>,
) -> Result<(), FileError> {
    let perm_entry = PermissionEntry::new(Some(user.into()), Some(group.into()), Some(mode));
    perm_entry.create_file(file.as_ref(), default_content).await
}

/// Moves a file to a destination path.
///
/// If source and destination are located on the same filesystem, a rename will
/// be used to avoid rewriting the file. If they are on different filesystems,
/// copy and delete method will be used instead.
///
/// Function cannot move whole directories if copy and delete method is used.
///
/// If the destination directory does not exist, it will be created, as well as
/// all parent directories.
///
/// This method returns
/// - `Ok(())` when file was moved successfully
/// - `Err(_)` when the source path does not exists or function has no
///   permission to move file
pub async fn move_file(
    src_path: impl AsRef<Path>,
    dest_path: impl AsRef<Path>,
    new_file_permissions: PermissionEntry,
) -> Result<(), FileMoveError> {
    let src_path = src_path.as_ref();
    let dest_path = dest_path.as_ref();

    if !path_exists(dest_path).await {
        if let Some(dir_to) = dest_path.parent() {
            tokio::fs::create_dir_all(dir_to)
                .await
                .map_err(|err| FileMoveError::new(src_path, dest_path, err))?;
            debug!("Created parent directories for {:?}", dest_path);
        }
    }

    let original_permission_mode = match dest_path.is_file() {
        true => {
            let metadata = get_metadata(src_path)
                .await
                .map_err(|err| FileMoveError::new(src_path, dest_path, err))?;
            let mode = metadata.permissions().mode();
            Some(mode)
        }
        false => None,
    };

    // Copy source to destination using rename. If that one fails due to cross-filesystem, use copy and delete.
    // As a ErrorKind::CrossesDevices is nightly feature we call copy and delete no matter what kind of error we get.
    tokio::fs::rename(src_path, dest_path)
        .or_else(|_| {
            tokio::fs::copy(src_path, dest_path).and_then(|_| tokio::fs::remove_file(src_path))
        })
        .await
        .map_err(|err| FileMoveError::new(src_path, dest_path, err))?;

    debug!("Moved file from {:?} to {:?}", src_path, dest_path);

    let file_permissions = if let Some(mode) = original_permission_mode {
        // Use the same file permission as the original one
        PermissionEntry::new(None, None, Some(mode))
    } else {
        // Set the user, group, and mode as given for a new file
        new_file_permissions
    };

    file_permissions
        .clone()
        .apply(dest_path)
        .await
        .map_err(|err| FileMoveError::new(src_path, dest_path, err))?;
    debug!(
        "Applied permissions: {:?} to {:?}",
        file_permissions, dest_path
    );

    Ok(())
}

#[derive(Debug, thiserror::Error)]
#[error("Could not move file from {src:?} to {dest:?}")]
pub struct FileMoveError {
    src: Box<Path>,
    dest: Box<Path>,
    source: anyhow::Error,
}

impl FileMoveError {
    fn new(
        src_path: &Path,
        dest_path: &Path,
        source_err: impl std::error::Error + Send + Sync + 'static,
    ) -> FileMoveError {
        FileMoveError {
            src: Box::from(src_path),
            dest: Box::from(dest_path),
            source: anyhow::Error::from(source_err),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Default, Clone)]
pub struct PermissionEntry {
    pub user: Option<String>,
    pub group: Option<String>,
    pub mode: Option<u32>,
}

impl PermissionEntry {
    pub fn new(user: Option<String>, group: Option<String>, mode: Option<u32>) -> Self {
        Self { user, group, mode }
    }

    pub async fn apply(self, path: &Path) -> Result<(), FileError> {
        match (self.user, self.group) {
            (Some(user), Some(group)) => {
                change_user_and_group(path.to_owned(), user, group).await?;
            }
            (Some(user), None) => {
                change_user(path.to_owned(), user).await?;
            }
            (None, Some(group)) => {
                change_group(path.to_owned(), group).await?;
            }
            (None, None) => {}
        }

        if let Some(mode) = &self.mode {
            change_mode(path, *mode).await?;
        }

        Ok(())
    }

    async fn create_directory(&self, dir: &Path) -> Result<(), FileError> {
        match dir.parent() {
            None => return Ok(()),
            Some(parent) => {
                if !path_exists(parent).await {
                    Box::pin(self.create_directory(parent)).await?;
                }
            }
        }
        debug!("Creating the directory {:?}", dir);
        let dir = dir.to_owned();
        match fs::create_dir(&dir).await {
            Ok(_) => {
                debug!(
                    "Applying desired user and group for newly created dir: {:?}",
                    dir
                );
                self.clone().apply(&dir).await?;
                Ok(())
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                debug!(
                    "Applying desired user and group for already existing dir: {:?}",
                    dir
                );
                self.clone().apply(&dir).await?;
                Ok(())
            }
            Err(e) => Err(FileError::DirectoryCreateFailed {
                dir: dir.display().to_string(),
                from: e,
            }),
        }
    }

    /// This function creates a file with a given path, specific access privileges and with the given content.
    /// If the file already exists, then it will not be re-created and it will not overwrite/append the contents of the file.
    /// This method returns
    ///     Ok() when file is created and the content is written successfully into the file.
    ///     Ok() when the file already exists
    ///     Err(_) When it can not create the file with the appropriate owner and access permissions.
    async fn create_file(
        &self,
        file: &Path,
        default_content: Option<&str>,
    ) -> Result<(), FileError> {
        let mut options = fs::OpenOptions::new();
        match options.create_new(true).write(true).open(file).await {
            Ok(mut f) => {
                self.clone().apply(file).await?;
                if let Some(default_content) = default_content {
                    f.write_all(default_content.as_bytes())
                        .map_err(|e| FileError::WriteContentFailed {
                            file: file.display().to_string(),
                            from: e,
                        })
                        .await?;
                    f.flush().await?;
                }
                f.sync_all().await.map_err(|from| FileError::FailedToSync {
                    file: file.to_path_buf(),
                    from,
                })?;
                Ok(())
            }

            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
            Err(e) => Err(FileError::FileCreateFailed {
                file: file.display().to_string(),
                from: e,
            }),
        }
    }
}

/// Overwrite the content of existing file. The file permissions will be kept.
pub async fn overwrite_file(file: &Path, content: &str) -> Result<(), FileError> {
    match fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(file)
        .await
    {
        Ok(mut f) => {
            f.write_all(content.as_bytes())
                .map_err(|e| FileError::WriteContentFailed {
                    file: file.display().to_string(),
                    from: e,
                })
                .await?;
            f.flush().await?;
            f.sync_all()
                .map_err(|from| FileError::FailedToSync {
                    file: file.to_path_buf(),
                    from,
                })
                .await?;
            Ok(())
        }
        Err(e) => Err(FileError::FileCreateFailed {
            file: file.display().to_string(),
            from: e,
        }),
    }
}

pub async fn change_user_and_group(
    file: PathBuf,
    user: String,
    group: String,
) -> Result<(), FileError> {
    let file = file.to_owned();
    let user = user.to_owned();
    let group = group.to_owned();
    tokio::task::spawn_blocking(move || change_user_and_group_sync(&file, &user, &group))
        .await
        .unwrap()
}

pub fn change_user_and_group_sync(file: &Path, user: &str, group: &str) -> Result<(), FileError> {
    let metadata = get_metadata_sync(file)?;
    debug!("Changing ownership of file: {file:?} with user: {user} and group: {group}",);
    let ud = get_user_by_name(&user)
        .map(|u| u.uid())
        .ok_or_else(|| FileError::UserNotFound {
            user: user.to_owned(),
        })?;

    let uid = metadata.uid();

    let gd =
        get_group_by_name(&group)
            .map(|g| g.gid())
            .ok_or_else(|| FileError::GroupNotFound {
                group: group.to_owned(),
            })?;

    let gid = metadata.gid();

    // if user and group are same as existing, then do not change
    if (ud != uid) || (gd != gid) {
        chown(file, Some(Uid::from_raw(ud)), Some(Gid::from_raw(gd))).map_err(|e| {
            FileError::MetaDataError {
                name: file.display().to_string(),
                from: e.into(),
            }
        })?;
    }

    Ok(())
}

async fn change_user(file: PathBuf, user: String) -> Result<(), FileError> {
    tokio::task::spawn_blocking(move || change_user_sync(&file, &user))
        .await
        .unwrap()
}

fn change_user_sync(file: &Path, user: &str) -> Result<(), FileError> {
    let metadata = get_metadata_sync(file)?;
    let ud = get_user_by_name(&user)
        .map(|u| u.uid())
        .ok_or_else(|| FileError::UserNotFound { user: user.into() })?;

    let uid = metadata.uid();

    // if user is same as existing, then do not change
    if ud != uid {
        chown(file, Some(Uid::from_raw(ud)), None).map_err(|e| FileError::MetaDataError {
            name: file.display().to_string(),
            from: e.into(),
        })?;
    }

    Ok(())
}

async fn change_group(file: impl Into<PathBuf>, group: impl Into<String>) -> Result<(), FileError> {
    let file = file.into();
    let group = group.into();
    tokio::task::spawn_blocking(move || change_group_sync(file, group))
        .await
        .unwrap()
}

fn change_group_sync(file: impl Into<PathBuf>, group: impl Into<String>) -> Result<(), FileError> {
    let file = file.into();
    let group = group.into();
    let metadata = get_metadata_sync(&file)?;
    let gd = get_group_by_name(&group)
        .map(|g| g.gid())
        .ok_or_else(|| FileError::GroupNotFound { group })?;

    let gid = metadata.gid();

    // if group is same as existing, then do not change
    if gd != gid {
        chown(&file, None, Some(Gid::from_raw(gd))).map_err(|e| FileError::MetaDataError {
            name: file.display().to_string(),
            from: e.into(),
        })?;
    }

    Ok(())
}

pub async fn change_mode(file: &Path, mode: u32) -> Result<(), FileError> {
    let file = file.to_owned();
    tokio::task::spawn_blocking(move || change_mode_sync(&file, mode))
        .await
        .unwrap()
}

pub fn change_mode_sync(file: &Path, mode: u32) -> Result<(), FileError> {
    let mut permissions = get_metadata_sync(Path::new(file))?.permissions();

    if permissions.mode() & 0o777 != mode {
        permissions.set_mode(mode);
        debug!("Setting mode of {} to {mode:0o}", file.display());
        std::fs::set_permissions(file, permissions).map_err(|e| FileError::ChangeModeError {
            name: file.display().to_string(),
            from: e,
        })
    } else {
        debug!(
            "Not changing mode of {} as it is already {mode:0o}",
            file.display()
        );
        Ok(())
    }
}

/// Return metadata when the given path exists and accessible by user
pub async fn get_metadata(path: &Path) -> Result<std::fs::Metadata, FileError> {
    fs::metadata(path)
        .await
        .map_err(|_| FileError::PathNotAccessible {
            path: path.to_path_buf(),
        })
}

fn get_metadata_sync(path: &Path) -> Result<std::fs::Metadata, FileError> {
    std::fs::metadata(path).map_err(|_| FileError::PathNotAccessible {
        path: path.to_path_buf(),
    })
}

/// Return filename if the given path contains a filename
pub fn get_filename(path: PathBuf) -> Option<String> {
    let filename = path.file_name()?.to_str()?.to_string();
    Some(filename)
}

/// Get uid from the user name
pub fn get_uid_by_name(user: &str) -> Result<u32, FileError> {
    let ud = get_user_by_name(user)
        .map(|u| u.uid())
        .ok_or_else(|| FileError::UserNotFound { user: user.into() })?;
    Ok(ud)
}

/// Get gid from the group name
pub fn get_gid_by_name(group: &str) -> Result<u32, FileError> {
    let gd = get_group_by_name(group)
        .map(|g| g.gid())
        .ok_or_else(|| FileError::GroupNotFound {
            group: group.into(),
        })?;
    Ok(gd)
}

pub async fn create_symlink(
    original: impl AsRef<Path>,
    link: impl AsRef<Path>,
) -> Result<(), FileError> {
    let link = link.as_ref();
    match fs::symlink(&original, &link).await {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => match fs::read_link(&link).await {
            Ok(path) if path == original.as_ref() => Ok(()),
            Ok(_) => Err(FileError::CreateSymlinkFailed {
                link: link.to_owned(),
                source: Error::other(format!(
                    "symlink exists but does not point to {:?}",
                    original.as_ref()
                )),
            }),
            Err(e) => Err(FileError::CreateSymlinkFailed {
                link: link.to_owned(),
                source: e,
            }),
        },
        Err(e) => Err(FileError::CreateSymlinkFailed {
            link: link.to_owned(),
            source: e,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use once_cell::sync::Lazy;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use tempfile::TempDir;

    static USER: Lazy<String> = Lazy::new(whoami::username);
    static GROUP: Lazy<String> = Lazy::new(|| {
        uzers::get_current_groupname()
            .unwrap()
            .into_string()
            .unwrap()
    });

    #[tokio::test]
    async fn create_file_correct_user_group() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("file").display().to_string();

        create_file_with_user_group(&file_path, &USER, &GROUP, 0o644, None)
            .await
            .unwrap();
        assert!(path_exists(file_path.as_str()).await);
        let meta = std::fs::metadata(file_path.as_str()).unwrap();
        let perm = meta.permissions();
        println!("{:o}", perm.mode());
        assert!(format!("{:o}", perm.mode()).contains("644"));
    }

    #[tokio::test]
    async fn create_file_with_default_content() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("file").display().to_string();

        let example_config = r#"# Add the configurations to be managed
        files = [
        #    { path = '/etc/tedge/tedge.toml' },
        ]"#;

        // Create a new file with default content
        create_file_with_user_group(&file_path, &USER, &GROUP, 0o775, Some(example_config))
            .await
            .unwrap();

        let content = fs::read(file_path).await.unwrap();
        assert_eq!(example_config.as_bytes(), content);
    }

    #[tokio::test]
    async fn create_file_wrong_user() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("file").display().to_string();

        let err = create_file_with_user_group(file_path, "nonexistent_user", &GROUP, 0o775, None)
            .await
            .unwrap_err();

        assert!(err.to_string().contains("User not found"));
    }

    #[tokio::test]
    async fn create_file_wrong_group() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("file").display().to_string();

        let err = create_file_with_user_group(&file_path, &USER, "nonexistent_group", 0o775, None)
            .await
            .unwrap_err();

        assert!(err.to_string().contains("Group not found"));
        fs::remove_file(&file_path).await.unwrap();
    }

    #[tokio::test]
    async fn create_directory_with_correct_user_group() {
        let temp_dir = TempDir::new().unwrap();
        let dir_path = temp_dir.path().join("dir").display().to_string();

        create_directory_with_user_group(&dir_path, &USER, &GROUP, 0o775)
            .await
            .unwrap();

        assert!(path_exists(&dir_path).await);
        let meta = fs::metadata(&dir_path).await.unwrap();
        let perm = meta.permissions();
        println!("{:o}", perm.mode());
        assert!(format!("{:o}", perm.mode()).contains("775"));
    }

    #[tokio::test]
    async fn create_directory_with_wrong_user() {
        let temp_dir = TempDir::new().unwrap();
        let dir_path = temp_dir.path().join("dir").display().to_string();

        let err = create_directory_with_user_group(dir_path, "nonexistent_user", &GROUP, 0o775)
            .await
            .unwrap_err();

        assert!(err.to_string().contains("User not found"));
    }

    #[tokio::test]
    async fn create_directory_with_wrong_group() {
        let temp_dir = TempDir::new().unwrap();
        let dir_path = temp_dir.path().join("dir").display().to_string();

        let err = create_directory_with_user_group(dir_path, &USER, "nonexistent_group", 0o775)
            .await
            .unwrap_err();

        assert!(err.to_string().contains("Group not found"));
    }

    #[tokio::test]
    async fn change_file_permissions() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("file").display().to_string();

        create_file_with_user_group(&file_path, &USER, &GROUP, 0o644, None)
            .await
            .unwrap();
        assert!(path_exists(&file_path).await);

        let meta = fs::metadata(&file_path).await.unwrap();
        let perm = meta.permissions();
        assert!(format!("{:o}", perm.mode()).contains("644"));

        let permission_set = PermissionEntry::new(None, None, Some(0o444));
        permission_set.apply(Path::new(&file_path)).await.unwrap();

        let meta = fs::metadata(&file_path).await.unwrap();
        let perm = meta.permissions();
        assert!(format!("{:o}", perm.mode()).contains("444"));
    }

    #[tokio::test]
    async fn verify_get_file_name() {
        assert_eq!(
            get_filename(PathBuf::from("/it/is/file.txt")),
            Some("file.txt".to_string())
        );
        assert_eq!(get_filename(PathBuf::from("/")), None);
    }

    #[tokio::test]
    async fn overwrite_file_content() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("file");
        create_file_with_user_group(&file_path, &USER, &GROUP, 0o775, None)
            .await
            .unwrap();

        let new_content = "abc";
        overwrite_file(file_path.as_path(), new_content)
            .await
            .unwrap();

        let actual_content = fs::read(file_path).await.unwrap();
        assert_eq!(actual_content, new_content.as_bytes());
    }

    #[test]
    fn get_uid_of_users() {
        assert_eq!(get_uid_by_name("root").unwrap(), 0);
        let err = get_uid_by_name("nonexistent_user").unwrap_err();
        assert!(err.to_string().contains("User not found"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn get_gid_of_groups() {
        assert_eq!(get_gid_by_name("root").unwrap(), 0);
        let err = get_gid_by_name("nonexistent_group").unwrap_err();
        assert!(err.to_string().contains("Group not found"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn get_gid_of_groups() {
        assert_ne!(get_gid_by_name("staff").unwrap(), 0);
        let err = get_gid_by_name("nonexistent_group").unwrap_err();
        assert!(err.to_string().contains("Group not found"));
    }

    #[tokio::test]
    async fn create_new_symlink() {
        let temp_dir = TempDir::new().unwrap();
        let source_path = temp_dir.path().join("source_file").display().to_string();
        let another_source_path = temp_dir
            .path()
            .join("another_source_file")
            .display()
            .to_string();
        let invalid_source_path = temp_dir.path().join("invalid_file").display().to_string();
        let dest_path = temp_dir.path().join("dest_file").display().to_string();

        // create symlink
        create_file_with_user_group(&source_path, &USER, &GROUP, 0o644, None)
            .await
            .unwrap();
        assert!(path_exists(&source_path).await);
        create_symlink(&source_path, &dest_path).await.unwrap();
        assert!(path_exists(&dest_path).await);

        // creating symlink again should not return error if source is the same
        assert!(create_symlink(&source_path, &dest_path).await.is_ok());

        // creating symlink again should  return error if source is different
        create_file_with_user_group(&another_source_path, &USER, &GROUP, 0o644, None)
            .await
            .unwrap();
        assert!(path_exists(&another_source_path).await);
        let err = create_symlink(&another_source_path, &dest_path)
            .await
            .unwrap_err();
        assert!(
            err.to_string()
                .contains("symlink exists but does not point to")
                && err.to_string().contains(another_source_path.as_str())
        );

        // creating symlink should not be possible to file that does not exists
        assert!(create_symlink(invalid_source_path, dest_path)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn move_file_to_different_filesystem() {
        let file_dir = TempDir::new().unwrap();
        let file_path = file_dir.path().join("file");
        create_file_with_user_group(&file_path, &USER, &GROUP, 0o775, Some("test"))
            .await
            .unwrap();

        let dest_dir = TempDir::new_in(".").unwrap();
        let dest_path = dest_dir.path().join("another-file");

        move_file(file_path, &dest_path, PermissionEntry::default())
            .await
            .unwrap();

        let content = fs::read(&dest_path).await.unwrap();
        assert_eq!("test".as_bytes(), content);
    }
}
