use camino::Utf8Path;
use camino::Utf8PathBuf;
use futures::TryFutureExt;
use nix::unistd::*;
use std::io::Error;
use std::os::unix::fs::MetadataExt;
use std::os::unix::fs::PermissionsExt;
use tokio::fs;
use tokio::io;
use tokio::io::AsyncWriteExt as _;
use tracing::debug;
use uzers::get_group_by_name;
use uzers::get_user_by_name;

#[derive(thiserror::Error, Debug)]
pub enum FileError {
    #[error("Creating the directory failed: {dir:?}. Reason: {from}")]
    DirectoryCreateFailed {
        dir: Utf8PathBuf,
        from: std::io::Error,
    },

    #[error("Creating the file failed: {file:?}. Reason: {from}")]
    FileCreateFailed {
        file: Utf8PathBuf,
        from: std::io::Error,
    },

    #[error("Failed to change owner: {name:?}. Reason: {from}")]
    MetaDataError { name: String, from: std::io::Error },

    #[error("Failed to change permissions of file: {name:?}. Reason: {from}")]
    ChangeModeError { name: String, from: std::io::Error },

    #[error("User not found: {user:?}.")]
    UserNotFound { user: String },

    #[error("Group not found: {group:?}.")]
    GroupNotFound { group: String },

    #[error("The path is not accessible. {path:?}")]
    PathNotAccessible { path: Utf8PathBuf },

    #[error("Writing the content to the file failed: {file:?}. Reason: {from}")]
    WriteContentFailed {
        file: Utf8PathBuf,
        from: std::io::Error,
    },

    #[error("Could not save the file {file:?} to disk. Received error: {from:?}.")]
    FailedToSync {
        file: Utf8PathBuf,
        from: std::io::Error,
    },

    #[error("The path {path:?} is invalid")]
    InvalidFileName {
        path: Utf8PathBuf,
        source: anyhow::Error,
    },

    #[error(transparent)]
    FromIoError(#[from] std::io::Error),

    #[error(transparent)]
    FileMove(#[from] FileMoveError),

    #[error("Failed to create a symlink {link:?}: {source:?}.")]
    CreateSymlinkFailed {
        link: Utf8PathBuf,
        source: std::io::Error,
    },
}

pub async fn path_exists(path: impl AsRef<Utf8Path>) -> bool {
    tokio::fs::try_exists(path.as_ref()).await.unwrap_or(false)
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
    src_path: impl AsRef<Utf8Path>,
    dest_path: impl AsRef<Utf8Path>,
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
    src: Utf8PathBuf,
    dest: Utf8PathBuf,
    source: anyhow::Error,
}

impl FileMoveError {
    fn new(
        src_path: &Utf8Path,
        dest_path: &Utf8Path,
        source_err: impl std::error::Error + Send + Sync + 'static,
    ) -> FileMoveError {
        FileMoveError {
            src: src_path.to_owned(),
            dest: dest_path.to_owned(),
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

    pub(crate) async fn apply(&self, path: impl AsRef<Utf8Path>) -> Result<(), FileError> {
        let path = path.as_ref();
        match (&self.user, &self.group) {
            (Some(user), Some(group)) => {
                change_user_and_group(path, user, group).await?;
            }
            (Some(user), None) => {
                change_user(path.to_owned(), user.clone()).await?;
            }
            (None, Some(group)) => {
                change_group(path.to_owned(), group.clone()).await?;
            }
            (None, None) => {}
        }

        if let Some(mode) = &self.mode {
            change_mode(path.to_owned(), *mode).await?;
        }

        Ok(())
    }

    pub fn apply_sync(&self, path: impl AsRef<Utf8Path>) -> Result<(), FileError> {
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
        dir: impl AsRef<Utf8Path>,
        root: impl AsRef<Utf8Path>,
    ) -> Result<(), FileError> {
        self.create_directory_internal(dir.as_ref(), Some(root.as_ref()))
            .await
    }

    async fn create_directory_internal(
        &self,
        dir: &Utf8Path,
        root: Option<&Utf8Path>,
    ) -> Result<(), FileError> {
        if let Some(root_dir) = root {
            ensure_parents_exist(root_dir).await?;
        }
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
            Err(e) => Err(FileError::DirectoryCreateFailed { dir, from: e }),
        }
    }
}

async fn ensure_parents_exist(dir: &Utf8Path) -> Result<(), FileError> {
    if let Some(parent) = dir.parent() {
        if !path_exists(parent).await {
            Box::pin(ensure_parents_exist(parent)).await?;

            if let Err(e) = fs::create_dir(parent).await {
                if e.kind() != io::ErrorKind::AlreadyExists {
                    return Err(FileError::DirectoryCreateFailed {
                        dir: parent.to_owned(),
                        from: e,
                    });
                }
            }
        }
    }
    Ok(())
}

/// Overwrite the content of existing file. The file permissions will be kept.
pub async fn overwrite_file(file: impl AsRef<Utf8Path>, content: &str) -> Result<(), FileError> {
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
                    file: file.to_owned(),
                    from: e,
                })
                .await?;
            f.flush().await?;
            f.sync_all()
                .map_err(|from| FileError::FailedToSync {
                    file: file.to_owned(),
                    from,
                })
                .await?;
            Ok(())
        }
        Err(e) => Err(FileError::FileCreateFailed {
            file: file.to_owned(),
            from: e,
        }),
    }
}

pub async fn change_user_and_group(
    file: impl AsRef<Utf8Path>,
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

fn change_user_and_group_sync(path: &Utf8Path, user: &str, group: &str) -> Result<(), FileError> {
    match (user, group) {
        ("", "") => return Ok(()),
        ("", group) => return change_group_sync(path, group),
        (user, "") => return change_user_sync(path, user),
        _ => {}
    }
    let metadata = get_metadata_sync(path)?;
    debug!("Changing ownership of path: {path:?} with user: {user} and group: {group}",);
    let ud = get_user_by_name(user)
        .map(|u| u.uid())
        .ok_or_else(|| FileError::UserNotFound {
            user: user.to_owned(),
        })?;

    let uid = metadata.uid();

    let gd = get_group_by_name(group)
        .map(|g| g.gid())
        .ok_or_else(|| FileError::GroupNotFound {
            group: group.to_owned(),
        })?;

    let gid = metadata.gid();

    if (ud != uid) || (gd != gid) {
        chown(
            path.as_std_path(),
            Some(Uid::from_raw(ud)),
            Some(Gid::from_raw(gd)),
        )
        .map_err(|e| FileError::MetaDataError {
            name: path.to_string(),
            from: e.into(),
        })?;
    }

    Ok(())
}

async fn change_user(file: Utf8PathBuf, user: String) -> Result<(), FileError> {
    tokio::task::spawn_blocking(move || change_user_sync(&file, &user))
        .await
        .unwrap()
}

fn change_user_sync(file: &Utf8Path, user: &str) -> Result<(), FileError> {
    let metadata = get_metadata_sync(file)?;
    let ud = get_user_by_name(user)
        .map(|u| u.uid())
        .ok_or_else(|| FileError::UserNotFound { user: user.into() })?;

    let uid = metadata.uid();

    if ud != uid {
        chown(file.as_std_path(), Some(Uid::from_raw(ud)), None).map_err(|e| {
            FileError::MetaDataError {
                name: file.to_string(),
                from: e.into(),
            }
        })?;
    }

    Ok(())
}

async fn change_group(file: Utf8PathBuf, group: String) -> Result<(), FileError> {
    tokio::task::spawn_blocking(move || change_group_sync(&file, &group))
        .await
        .unwrap()
}

fn change_group_sync(file: &Utf8Path, group: &str) -> Result<(), FileError> {
    let metadata = get_metadata_sync(file)?;
    let gd = get_group_by_name(group)
        .map(|g| g.gid())
        .ok_or_else(|| FileError::GroupNotFound {
            group: group.to_owned(),
        })?;

    let gid = metadata.gid();

    if gd != gid {
        chown(file.as_std_path(), None, Some(Gid::from_raw(gd))).map_err(|e| {
            FileError::MetaDataError {
                name: file.to_string(),
                from: e.into(),
            }
        })?;
    }

    Ok(())
}

async fn change_mode(file: Utf8PathBuf, mode: u32) -> Result<(), FileError> {
    tokio::task::spawn_blocking(move || change_mode_sync(&file, mode))
        .await
        .unwrap()
}

fn change_mode_sync(file: &Utf8Path, mode: u32) -> Result<(), FileError> {
    let mut permissions = get_metadata_sync(file)?.permissions();

    if permissions.mode() & 0o777 != mode {
        permissions.set_mode(mode);
        debug!("Setting mode of {file} to {mode:0o}");
        std::fs::set_permissions(file, permissions).map_err(|e| FileError::ChangeModeError {
            name: file.to_string(),
            from: e,
        })
    } else {
        debug!("Not changing mode of {file} as it is already {mode:0o}");
        Ok(())
    }
}

async fn get_metadata(path: &Utf8Path) -> Result<std::fs::Metadata, FileError> {
    fs::metadata(path)
        .await
        .map_err(|_| FileError::PathNotAccessible {
            path: path.to_owned(),
        })
}

fn get_metadata_sync(path: &Utf8Path) -> Result<std::fs::Metadata, FileError> {
    std::fs::metadata(path).map_err(|_| FileError::PathNotAccessible {
        path: path.to_owned(),
    })
}

pub async fn create_symlink(
    original: impl AsRef<Utf8Path>,
    link: impl AsRef<Utf8Path>,
) -> Result<(), FileError> {
    let original = original.as_ref();
    let link = link.as_ref();
    match fs::symlink(original, link).await {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => match fs::read_link(link).await {
            Ok(path) if path.as_path() == original => Ok(()),
            Ok(_) => Err(FileError::CreateSymlinkFailed {
                link: link.to_owned(),
                source: Error::other(format!("symlink exists but does not point to {original:?}")),
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

        TedgePaths::from_root_with_defaults(ttd.path(), "", "")
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

        TedgePaths::from_root_with_defaults(src_dir.path(), "", "")
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
