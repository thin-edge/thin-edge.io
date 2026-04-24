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
    #[error("Creating the directory failed: {dir:?}. Reason: {from}")]
    DirectoryCreateFailed { dir: String, from: std::io::Error },

    #[error("Creating the file failed: {file:?}. Reason: {from}")]
    FileCreateFailed { file: String, from: std::io::Error },

    #[error("Failed to change owner: {name:?}. Reason: {from}")]
    MetaDataError { name: String, from: std::io::Error },

    #[error("Failed to change permissions of file: {name:?}. Reason: {from}")]
    ChangeModeError { name: String, from: std::io::Error },

    #[error("User not found: {user:?}.")]
    UserNotFound { user: String },

    #[error("Group not found: {group:?}.")]
    GroupNotFound { group: String },

    #[error("The path is not accessible. {path:?}")]
    PathNotAccessible { path: PathBuf },

    #[error("Writing the content to the file failed: {file:?}. Reason: {from}")]
    WriteContentFailed { file: String, from: std::io::Error },

    #[error("Could not save the file {file:?} to disk. Received error: {from:?}.")]
    FailedToSync { file: PathBuf, from: std::io::Error },

    #[error("The path {path:?} is invalid")]
    InvalidFileName {
        path: PathBuf,
        // Can be io::Error or Utf8Error
        source: anyhow::Error,
    },

    #[error(transparent)]
    FromIoError(#[from] std::io::Error),

    #[error(transparent)]
    FileMove(#[from] FileMoveError),

    #[error("Failed to create a symlink {link:?}: {source:?}.")]
    CreateSymlinkFailed {
        link: PathBuf,
        source: std::io::Error,
    },
}

pub async fn path_exists(path: impl AsRef<Path>) -> bool {
    tokio::fs::try_exists(path).await.unwrap_or(false)
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
    pub reassert_dir_ownership: bool,
}

impl PermissionEntry {
    pub fn new(user: Option<String>, group: Option<String>, mode: Option<u32>) -> Self {
        Self {
            user,
            group,
            mode,
            reassert_dir_ownership: false,
        }
    }

    pub(crate) fn force_dir_ownership(mut self) -> Self {
        self.reassert_dir_ownership = true;
        self
    }

    pub(crate) async fn apply(&self, path: impl AsRef<Path>) -> Result<(), FileError> {
        let path = path.as_ref();
        match (&self.user, &self.group) {
            (Some(user), Some(group)) => {
                change_user_and_group(path, user, group).await?;
            }
            (Some(user), None) => {
                change_user(path, user).await?;
            }
            (None, Some(group)) => {
                change_group(path, group).await?;
            }
            (None, None) => {}
        }

        if let Some(mode) = &self.mode {
            change_mode(path, *mode).await?;
        }

        Ok(())
    }

    pub fn apply_sync(&self, path: impl AsRef<Path>) -> Result<(), FileError> {
        let path = path.as_ref();
        match (&self.user, &self.group) {
            (Some(user), Some(group)) => {
                change_user_and_group_sync(path, user, group)?;
            }
            (Some(user), None) => {
                change_user_sync(path, user)?;
            }
            (None, Some(group)) => {
                change_group_sync(path, group)?;
            }
            (None, None) => {}
        }

        if let Some(mode) = &self.mode {
            change_mode_sync(path, *mode)?;
        }

        Ok(())
    }

    pub(crate) async fn create_directory_with_root(
        &self,
        dir: impl AsRef<Path>,
        root: impl AsRef<Path>,
    ) -> Result<(), FileError> {
        self.create_directory_internal(dir.as_ref(), Some(root.as_ref()))
            .await
    }

    async fn create_directory_internal(
        &self,
        dir: &Path,
        root: Option<&Path>,
    ) -> Result<(), FileError> {
        match dir.parent() {
            None => return Ok(()),
            Some(_parent) if Some(dir) == root => {}
            Some(parent) => {
                if !path_exists(parent).await {
                    Box::pin(self.create_directory_internal(parent, root)).await?;
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
                self.apply(&dir).await?;
                Ok(())
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                if self.reassert_dir_ownership {
                    debug!(
                        "Updating user and group for already existing dir: {:?}",
                        dir
                    );
                    self.apply(&dir).await?;
                }
                Ok(())
            }
            Err(e) => Err(FileError::DirectoryCreateFailed {
                dir: dir.display().to_string(),
                from: e,
            }),
        }
    }
}

/// Overwrite the content of existing file. The file permissions will be kept.
pub async fn overwrite_file(file: impl AsRef<Path>, content: &str) -> Result<(), FileError> {
    let file = file.as_ref();
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
    file: impl AsRef<Path>,
    user: impl Into<String>,
    group: impl Into<String>,
) -> Result<(), FileError> {
    let file = file.as_ref().to_owned();
    let user = user.into();
    let group = group.into();
    tokio::task::spawn_blocking(move || change_user_and_group_sync(&file, &user, &group))
        .await
        .unwrap()
}

fn change_user_and_group_sync(path: &Path, user: &str, group: &str) -> Result<(), FileError> {
    match (user, group) {
        ("", "") => return Ok(()),
        ("", group) => return change_group_sync(path, group),
        (user, "") => return change_user_sync(path, user),
        _ => {}
    }
    let metadata = get_metadata_sync(path)?;
    debug!("Changing ownership of path: {path:?} with user: {user} and group: {group}",);
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
        chown(path, Some(Uid::from_raw(ud)), Some(Gid::from_raw(gd))).map_err(|e| {
            FileError::MetaDataError {
                name: path.display().to_string(),
                from: e.into(),
            }
        })?;
    }

    Ok(())
}

async fn change_user(file: impl Into<PathBuf>, user: impl Into<String>) -> Result<(), FileError> {
    let file = file.into();
    let user = user.into();
    tokio::task::spawn_blocking(move || change_user_sync(&file, &user))
        .await
        .unwrap()
}

fn change_user_sync(file: impl AsRef<Path>, user: &str) -> Result<(), FileError> {
    let file = file.as_ref();
    let metadata = get_metadata_sync(file)?;
    let ud = get_user_by_name(user)
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
    tokio::task::spawn_blocking(move || change_group_sync(&file, &group))
        .await
        .unwrap()
}

fn change_group_sync(file: impl AsRef<Path>, group: &str) -> Result<(), FileError> {
    let file = file.as_ref();
    let metadata = get_metadata_sync(file)?;
    let gd = get_group_by_name(group)
        .map(|g| g.gid())
        .ok_or_else(|| FileError::GroupNotFound {
            group: group.to_owned(),
        })?;

    let gid = metadata.gid();

    // if group is same as existing, then do not change
    if gd != gid {
        chown(file, None, Some(Gid::from_raw(gd))).map_err(|e| FileError::MetaDataError {
            name: file.display().to_string(),
            from: e.into(),
        })?;
    }

    Ok(())
}

async fn change_mode(file: impl AsRef<Path>, mode: u32) -> Result<(), FileError> {
    let file = file.as_ref().to_owned();
    tokio::task::spawn_blocking(move || change_mode_sync(&file, mode))
        .await
        .unwrap()
}

fn change_mode_sync(file: impl AsRef<Path>, mode: u32) -> Result<(), FileError> {
    let file = file.as_ref();
    let mut permissions = get_metadata_sync(file)?.permissions();

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
async fn get_metadata(path: impl AsRef<Path>) -> Result<std::fs::Metadata, FileError> {
    let path = path.as_ref();
    fs::metadata(path)
        .await
        .map_err(|_| FileError::PathNotAccessible {
            path: path.to_path_buf(),
        })
}

fn get_metadata_sync(path: impl AsRef<Path>) -> Result<std::fs::Metadata, FileError> {
    let path = path.as_ref();
    std::fs::metadata(path).map_err(|_| FileError::PathNotAccessible {
        path: path.to_path_buf(),
    })
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
    use crate::paths::TedgePaths;
    use std::os::unix::fs::PermissionsExt;
    use tedge_test_utils::fs::TempTedgeDir;

    #[tokio::test]
    async fn change_file_permissions() {
        let ttd = TempTedgeDir::new();
        let file_path = ttd.path().join("file");

        TedgePaths::from_root_with_defaults(ttd.utf8_path(), "", "")
            .file("file")
            .unwrap()
            .with_mode(0o644)
            .create_if_missing("")
            .await
            .unwrap();

        let meta = fs::metadata(&file_path).await.unwrap();
        assert_eq!(meta.permissions().mode() & 0o777, 0o644);

        let permission_set = PermissionEntry::new(None, None, Some(0o444));
        permission_set.apply(&file_path).await.unwrap();

        let meta = fs::metadata(&file_path).await.unwrap();
        assert_eq!(meta.permissions().mode() & 0o777, 0o444);
    }

    #[tokio::test]
    async fn overwrite_file_content() {
        let ttd = TempTedgeDir::new();
        let file_path = ttd.path().join("file");

        ttd.file("file");

        overwrite_file(&file_path, "abc").await.unwrap();

        let actual = fs::read(&file_path).await.unwrap();
        assert_eq!(actual, b"abc");
    }

    #[tokio::test]
    async fn create_new_symlink() {
        let ttd = TempTedgeDir::new();
        let source_path = ttd.path().join("source_file");
        let another_source_path = ttd.path().join("another_source_file");
        let dest_path = ttd.path().join("dest_file");

        ttd.file("source_file");
        create_symlink(&source_path, &dest_path).await.unwrap();
        assert!(path_exists(&dest_path).await);

        // Idempotent when target is the same
        assert!(create_symlink(&source_path, &dest_path).await.is_ok());

        // Fails when an existing symlink points to a different target
        ttd.file("another_source_file");
        let err = create_symlink(&another_source_path, &dest_path)
            .await
            .unwrap_err();
        assert!(err
            .to_string()
            .contains("symlink exists but does not point to"));
    }

    #[tokio::test]
    async fn move_file_to_different_filesystem() {
        let src_dir = TempTedgeDir::new();
        let file_path = src_dir.path().join("file");

        TedgePaths::from_root_with_defaults(src_dir.utf8_path(), "", "")
            .file("file")
            .unwrap()
            .with_mode(0o775)
            .create_if_missing("test")
            .await
            .unwrap();

        let dest_dir = TempTedgeDir::new();
        let dest_path = dest_dir.path().join("another-file");

        move_file(&file_path, &dest_path, PermissionEntry::default())
            .await
            .unwrap();

        let content = fs::read(&dest_path).await.unwrap();
        assert_eq!(content, b"test");
    }
}
