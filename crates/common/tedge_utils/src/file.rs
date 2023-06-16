use futures::TryFutureExt;
use nix::unistd::*;
use std::fs;
use std::io;
use std::io::Write;
use std::os::linux::fs::MetadataExt;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;
use tracing::debug;
use users::get_group_by_name;
use users::get_user_by_name;

#[derive(thiserror::Error, Debug)]
pub enum FileError {
    #[error("Creating the directory failed: {dir:?}.")]
    DirectoryCreateFailed { dir: String, from: std::io::Error },

    #[error("Creating the file failed: {file:?}.")]
    FileCreateFailed { file: String, from: std::io::Error },

    #[error("Failed to change owner: {name:?}.")]
    MetaDataError { name: String, from: std::io::Error },

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

    #[error(transparent)]
    FromIoError(#[from] std::io::Error),

    #[error("No write access to {path:?}")]
    NoWriteAccess { path: PathBuf },

    #[error(transparent)]
    FileMove(#[from] FileMoveError),
}

pub fn create_directory<P: AsRef<Path>>(
    dir: P,
    permissions: PermissionEntry,
) -> Result<(), FileError> {
    permissions.create_directory(dir.as_ref())
}
/// Create the directory owned by the user running this API with default directory permissions
pub fn create_directory_with_defaults<P: AsRef<Path>>(dir: P) -> Result<(), FileError> {
    create_directory(dir, PermissionEntry::default())
}

pub fn create_directory_with_user_group(
    dir: impl AsRef<Path>,
    user: &str,
    group: &str,
    mode: u32,
) -> Result<(), FileError> {
    let perm_entry = PermissionEntry::new(Some(user.into()), Some(group.into()), Some(mode));
    perm_entry.create_directory(dir.as_ref())
}

pub fn create_directory_with_mode(dir: impl AsRef<Path>, mode: u32) -> Result<(), FileError> {
    let perm_entry = PermissionEntry::new(None, None, Some(mode));
    perm_entry.create_directory(dir.as_ref())
}

pub fn create_file<P: AsRef<Path>>(
    file: P,
    content: Option<&str>,
    permissions: PermissionEntry,
) -> Result<(), FileError> {
    permissions.create_file(file.as_ref(), content)
}

/// Create the directory owned by the user running this API with default file permissions
pub fn create_file_with_defaults<P: AsRef<Path>>(
    file: P,
    content: Option<&str>,
) -> Result<(), FileError> {
    create_file(file, content, PermissionEntry::default())
}

pub fn create_file_with_mode(
    file: impl AsRef<Path>,
    content: Option<&str>,
    mode: u32,
) -> Result<(), FileError> {
    let perm_entry = PermissionEntry::new(None, None, Some(mode));
    perm_entry.create_file(file.as_ref(), content)
}

pub fn create_file_with_user_group(
    file: impl AsRef<Path>,
    user: &str,
    group: &str,
    mode: u32,
    default_content: Option<&str>,
) -> Result<(), FileError> {
    let perm_entry = PermissionEntry::new(Some(user.into()), Some(group.into()), Some(mode));
    perm_entry.create_file(file.as_ref(), default_content)
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

    if !dest_path.exists() {
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

    pub fn apply(&self, path: &Path) -> Result<(), FileError> {
        match (&self.user, &self.group) {
            (Some(user), Some(group)) => {
                change_user_and_group(path, user, group)?;
            }
            (Some(user), None) => {
                change_user(path, user)?;
            }
            (None, Some(group)) => {
                change_group(path, group)?;
            }
            (None, None) => {}
        }

        if let Some(mode) = &self.mode {
            change_mode(path, *mode)?;
        }

        Ok(())
    }

    fn create_directory(&self, dir: &Path) -> Result<(), FileError> {
        match dir.parent() {
            None => return Ok(()),
            Some(parent) => {
                if !parent.exists() {
                    self.create_directory(parent)?;
                }
            }
        }
        debug!("Creating the directory {:?}", dir);
        match fs::create_dir(dir) {
            Ok(_) => {
                debug!(
                    "Applying desired user and group for newly created dir: {:?}",
                    dir
                );
                self.apply(dir)?;
                Ok(())
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                debug!(
                    "Applying desired user and group for already existing dir: {:?}",
                    dir
                );
                self.apply(dir)?;
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
    fn create_file(&self, file: &Path, default_content: Option<&str>) -> Result<(), FileError> {
        match fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(file)
        {
            Ok(mut f) => {
                self.apply(file)?;
                f.sync_all().map_err(|from| FileError::FailedToSync {
                    file: file.to_path_buf(),
                    from,
                })?;
                if let Some(default_content) = default_content {
                    f.write(default_content.as_bytes()).map_err(|e| {
                        FileError::WriteContentFailed {
                            file: file.display().to_string(),
                            from: e,
                        }
                    })?;
                }
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
pub fn overwrite_file(file: &Path, content: &str) -> Result<(), FileError> {
    match fs::OpenOptions::new().write(true).truncate(true).open(file) {
        Ok(mut f) => {
            f.sync_all().map_err(|from| FileError::FailedToSync {
                file: file.to_path_buf(),
                from,
            })?;
            f.write(content.as_bytes())
                .map_err(|e| FileError::WriteContentFailed {
                    file: file.display().to_string(),
                    from: e,
                })?;
            Ok(())
        }
        Err(e) => Err(FileError::FileCreateFailed {
            file: file.display().to_string(),
            from: e,
        }),
    }
}

pub fn change_user_and_group(file: &Path, user: &str, group: &str) -> Result<(), FileError> {
    debug!(
        "Changing ownership of file: {:?} with user: {} and group: {}",
        file, user, group
    );
    let ud = get_user_by_name(user)
        .map(|u| u.uid())
        .ok_or_else(|| FileError::UserNotFound { user: user.into() })?;

    let uid = get_metadata(Path::new(file))?.st_uid();

    let gd = get_group_by_name(group)
        .map(|g| g.gid())
        .ok_or_else(|| FileError::GroupNotFound {
            group: group.into(),
        })?;

    let gid = get_metadata(Path::new(file))?.st_gid();

    // if user and group are same as existing, then do not change
    if (ud != uid) && (gd != gid) {
        chown(file, Some(Uid::from_raw(ud)), Some(Gid::from_raw(gd))).map_err(|e| {
            FileError::MetaDataError {
                name: file.display().to_string(),
                from: e.into(),
            }
        })?;
    }

    Ok(())
}

fn change_user(file: &Path, user: &str) -> Result<(), FileError> {
    let ud = get_user_by_name(user)
        .map(|u| u.uid())
        .ok_or_else(|| FileError::UserNotFound { user: user.into() })?;

    let uid = get_metadata(Path::new(file))?.st_uid();

    // if user is same as existing, then do not change
    if ud != uid {
        chown(file, Some(Uid::from_raw(ud)), None).map_err(|e| FileError::MetaDataError {
            name: file.display().to_string(),
            from: e.into(),
        })?;
    }

    Ok(())
}

fn change_group(file: &Path, group: &str) -> Result<(), FileError> {
    let gd = get_group_by_name(group)
        .map(|g| g.gid())
        .ok_or_else(|| FileError::GroupNotFound {
            group: group.into(),
        })?;

    let gid = get_metadata(Path::new(file))?.st_gid();

    // if group is same as existing, then do not change
    if gd != gid {
        chown(file, None, Some(Gid::from_raw(gd))).map_err(|e| FileError::MetaDataError {
            name: file.display().to_string(),
            from: e.into(),
        })?;
    }

    Ok(())
}

fn change_mode(file: &Path, mode: u32) -> Result<(), FileError> {
    let mut perm = get_metadata(Path::new(file))?.permissions();
    perm.set_mode(mode);

    fs::set_permissions(file, perm).map_err(|e| FileError::MetaDataError {
        name: file.display().to_string(),
        from: e,
    })?;

    Ok(())
}

/// Return metadata when the given path exists and accessible by user
pub fn get_metadata(path: &Path) -> Result<fs::Metadata, FileError> {
    fs::metadata(path).map_err(|_| FileError::PathNotAccessible {
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

/// Return () if a file of the given file path
/// - already exists, and has a write permission.
/// - doesn't exist, but the parent directory has a write permission.
pub fn has_write_access(path: &Path) -> Result<(), FileError> {
    let metadata = if path.is_file() {
        get_metadata(path)?
    } else {
        let parent_dir = path.parent().ok_or_else(|| FileError::NoWriteAccess {
            path: path.to_path_buf(),
        })?;
        get_metadata(parent_dir)?
    };

    if metadata.permissions().readonly() {
        Err(FileError::NoWriteAccess {
            path: path.to_path_buf(),
        })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use tempfile::TempDir;

    #[test]
    fn create_file_correct_user_group() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("file").display().to_string();

        let user = whoami::username();
        create_file_with_user_group(&file_path, &user, &user, 0o644, None).unwrap();
        assert!(Path::new(file_path.as_str()).exists());
        let meta = std::fs::metadata(file_path.as_str()).unwrap();
        let perm = meta.permissions();
        println!("{:o}", perm.mode());
        assert!(format!("{:o}", perm.mode()).contains("644"));
    }

    #[test]
    fn create_file_with_default_content() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("file").display().to_string();
        let user = whoami::username();

        let example_config = r#"# Add the configurations to be managed by c8y-configuration-plugin
        files = [
        #    { path = '/etc/tedge/tedge.toml' },
        ]"#;

        // Create a new file with default content
        create_file_with_user_group(&file_path, &user, &user, 0o775, Some(example_config)).unwrap();

        let content = fs::read(file_path).unwrap();
        assert_eq!(example_config.as_bytes(), content);
    }

    #[test]
    fn create_file_wrong_user() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("file").display().to_string();

        let user = whoami::username();
        let err = create_file_with_user_group(file_path, "test", &user, 0o775, None).unwrap_err();

        assert!(err.to_string().contains("User not found"));
    }

    #[test]
    fn create_file_wrong_group() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("file").display().to_string();

        let user = whoami::username();
        let err = create_file_with_user_group(&file_path, &user, "test", 0o775, None).unwrap_err();

        assert!(err.to_string().contains("Group not found"));
        fs::remove_file(file_path.as_str()).unwrap();
    }

    #[test]
    fn create_directory_with_correct_user_group() {
        let temp_dir = TempDir::new().unwrap();
        let dir_path = temp_dir.path().join("dir").display().to_string();

        let user = whoami::username();
        create_directory_with_user_group(&dir_path, &user, &user, 0o775).unwrap();

        assert!(Path::new(dir_path.as_str()).exists());
        let meta = fs::metadata(dir_path.as_str()).unwrap();
        let perm = meta.permissions();
        println!("{:o}", perm.mode());
        assert!(format!("{:o}", perm.mode()).contains("775"));
    }

    #[test]
    fn create_directory_with_wrong_user() {
        let temp_dir = TempDir::new().unwrap();
        let dir_path = temp_dir.path().join("dir").display().to_string();

        let user = whoami::username();

        let err = create_directory_with_user_group(dir_path, "test", &user, 0o775).unwrap_err();

        assert!(err.to_string().contains("User not found"));
    }

    #[test]
    fn create_directory_with_wrong_group() {
        let temp_dir = TempDir::new().unwrap();
        let dir_path = temp_dir.path().join("dir").display().to_string();

        let user = whoami::username();

        let err = create_directory_with_user_group(dir_path, &user, "test", 0o775).unwrap_err();

        assert!(err.to_string().contains("Group not found"));
    }

    #[test]
    fn change_file_permissions() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("file").display().to_string();

        let user = whoami::username();
        create_file_with_user_group(&file_path, &user, &user, 0o644, None).unwrap();
        assert!(Path::new(file_path.as_str()).exists());

        let meta = fs::metadata(file_path.as_str()).unwrap();
        let perm = meta.permissions();
        assert!(format!("{:o}", perm.mode()).contains("644"));

        let permission_set = PermissionEntry::new(None, None, Some(0o444));
        permission_set.apply(Path::new(file_path.as_str())).unwrap();

        let meta = fs::metadata(file_path.as_str()).unwrap();
        let perm = meta.permissions();
        assert!(format!("{:o}", perm.mode()).contains("444"));
    }

    #[test]
    fn verify_get_file_name() {
        assert_eq!(
            get_filename(PathBuf::from("/it/is/file.txt")),
            Some("file.txt".to_string())
        );
        assert_eq!(get_filename(PathBuf::from("/")), None);
    }

    #[test]
    fn overwrite_file_content() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("file");
        let user = whoami::username();
        create_file_with_user_group(&file_path, &user, &user, 0o775, None).unwrap();

        let new_content = "abc";
        overwrite_file(file_path.as_path(), new_content).unwrap();

        let actual_content = fs::read(file_path).unwrap();
        assert_eq!(actual_content, new_content.as_bytes());
    }

    #[test]
    fn get_uid_of_users() {
        assert_eq!(get_uid_by_name("root").unwrap(), 0);
        let err = get_uid_by_name("test").unwrap_err();
        assert!(err.to_string().contains("User not found"));
    }

    #[test]
    fn get_gid_of_groups() {
        assert_eq!(get_gid_by_name("root").unwrap(), 0);
        let err = get_gid_by_name("test").unwrap_err();
        assert!(err.to_string().contains("Group not found"));
    }

    #[tokio::test]
    async fn move_file_to_different_filesystem() {
        let file_dir = TempDir::new().unwrap();
        let file_path = file_dir.path().join("file");
        let user = whoami::username();
        create_file_with_user_group(&file_path, &user, &user, 0o775, Some("test")).unwrap();

        let dest_dir = TempDir::new_in(".").unwrap();
        let dest_path = dest_dir.path().join("another-file");

        move_file(file_path, &dest_path, PermissionEntry::default())
            .await
            .unwrap();

        let content = fs::read(&dest_path).unwrap();
        assert_eq!("test".as_bytes(), content);
    }
}
