use nix::unistd::*;
use std::fs::File;
use std::os::linux::fs::MetadataExt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::{fs, io};
use users::{get_group_by_name, get_user_by_name};

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
}

pub fn create_directory_with_user_group(
    dir: &str,
    user: &str,
    group: &str,
    mode: u32,
) -> Result<(), FileError> {
    let perm_entry = PermissionEntry::new(Some(user.into()), Some(group.into()), Some(mode));
    let () = perm_entry.create_directory(Path::new(dir))?;
    Ok(())
}

pub fn create_directory_with_mode(dir: &str, mode: u32) -> Result<(), FileError> {
    let perm_entry = PermissionEntry::new(None, None, Some(mode));
    let () = perm_entry.create_directory(Path::new(dir))?;
    Ok(())
}

pub fn create_file_with_user_group(
    file: &str,
    user: &str,
    group: &str,
    mode: u32,
) -> Result<(), FileError> {
    let perm_entry = PermissionEntry::new(Some(user.into()), Some(group.into()), Some(mode));
    let () = perm_entry.create_file(Path::new(file))?;
    Ok(())
}

pub fn create_file_with_mode(file: &str, mode: u32) -> Result<(), FileError> {
    let perm_entry = PermissionEntry::new(None, None, Some(mode));
    let () = perm_entry.create_file(Path::new(file))?;
    Ok(())
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
                let () = change_user_and_group(path, user, group)?;
            }
            (Some(user), None) => {
                let () = change_user(path, user)?;
            }
            (None, Some(group)) => {
                let () = change_group(path, group)?;
            }
            (None, None) => {}
        }

        if let Some(mode) = &self.mode {
            let () = change_mode(path, *mode)?;
        }

        Ok(())
    }

    fn create_directory(&self, dir: &Path) -> Result<(), FileError> {
        match fs::create_dir(dir) {
            Ok(_) => {
                let () = self.apply(dir)?;
                Ok(())
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
            Err(e) => Err(FileError::DirectoryCreateFailed {
                dir: dir.display().to_string(),
                from: e,
            }),
        }
    }

    fn create_file(&self, file: &Path) -> Result<(), FileError> {
        match File::create(file) {
            Ok(_) => {
                let () = self.apply(file)?;
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

fn change_user_and_group(file: &Path, user: &str, group: &str) -> Result<(), FileError> {
    let ud = match get_user_by_name(user) {
        Some(user) => user.uid(),
        None => {
            return Err(FileError::UserNotFound { user: user.into() });
        }
    };
    let uid = get_metadata(Path::new(file))?.st_uid();

    let gd = match get_group_by_name(group) {
        Some(group) => group.gid(),
        None => {
            return Err(FileError::GroupNotFound {
                group: group.into(),
            });
        }
    };
    let gid = get_metadata(Path::new(file))?.st_gid();

    // if user and group are same as existing, then do not change
    if (ud != uid) && (gd != gid) {
        chown(file, Some(Uid::from_raw(ud)), Some(Gid::from_raw(gd)))?;
    }

    Ok(())
}

fn change_user(file: &Path, user: &str) -> Result<(), FileError> {
    let ud = match get_user_by_name(user) {
        Some(user) => user.uid(),
        None => {
            return Err(FileError::UserNotFound { user: user.into() });
        }
    };

    let uid = get_metadata(Path::new(file))?.st_uid();

    // if user is same as existing, then do not change
    if ud != uid {
        chown(file, Some(Uid::from_raw(ud)), None)?;
    }

    Ok(())
}

fn change_group(file: &Path, group: &str) -> Result<(), FileError> {
    let gd = match get_group_by_name(group) {
        Some(group) => group.gid(),
        None => {
            return Err(FileError::GroupNotFound {
                group: group.into(),
            });
        }
    };

    let gid = get_metadata(Path::new(file))?.st_gid();

    // if group is same as existing, then do not change
    if gd != gid {
        chown(file, None, Some(Gid::from_raw(gd)))?;
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
    fs::metadata(&path).map_err(|_| FileError::PathNotAccessible {
        path: path.to_path_buf(),
    })
}

/// Return filename if the given path contains a filename
pub fn get_filename(path: PathBuf) -> Option<String> {
    let filename = path.file_name()?.to_str()?.to_string();
    Some(filename)
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
        let _ = create_file_with_user_group(file_path.as_str(), &user, &user, 0o644).unwrap();
        assert!(Path::new(file_path.as_str()).exists());
        let meta = std::fs::metadata(file_path.as_str()).unwrap();
        let perm = meta.permissions();
        println!("{:o}", perm.mode());
        assert!(format!("{:o}", perm.mode()).contains("644"));
    }

    #[test]
    fn create_file_wrong_user() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("file").display().to_string();

        let user = whoami::username();
        let err =
            create_file_with_user_group(file_path.as_str(), "test", &user, 0o775).unwrap_err();

        assert!(err.to_string().contains("User not found"));
    }

    #[test]
    fn create_file_wrong_group() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("file").display().to_string();

        let user = whoami::username();
        let err =
            create_file_with_user_group(file_path.as_str(), &user, "test", 0o775).unwrap_err();

        assert!(err.to_string().contains("Group not found"));
        fs::remove_file(file_path.as_str()).unwrap();
    }

    #[test]
    fn create_directory_with_correct_user_group() {
        let temp_dir = TempDir::new().unwrap();
        let dir_path = temp_dir.path().join("dir").display().to_string();

        let user = whoami::username();
        let _ = create_directory_with_user_group(dir_path.as_str(), &user, &user, 0o775).unwrap();

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

        let err =
            create_directory_with_user_group(dir_path.as_str(), "test", &user, 0o775).unwrap_err();

        assert!(err.to_string().contains("User not found"));
    }

    #[test]
    fn create_directory_with_wrong_group() {
        let temp_dir = TempDir::new().unwrap();
        let dir_path = temp_dir.path().join("dir").display().to_string();

        let user = whoami::username();

        let err =
            create_directory_with_user_group(dir_path.as_str(), &user, "test", 0o775).unwrap_err();

        assert!(err.to_string().contains("Group not found"));
    }

    #[test]
    fn change_file_permissions() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("file").display().to_string();

        let user = whoami::username();
        let _ = create_file_with_user_group(file_path.as_str(), &user, &user, 0o644).unwrap();
        assert!(Path::new(file_path.as_str()).exists());

        let meta = fs::metadata(file_path.as_str()).unwrap();
        let perm = meta.permissions();
        assert!(format!("{:o}", perm.mode()).contains("644"));

        let permission_set = PermissionEntry::new(None, None, Some(0o444));
        let () = permission_set.apply(Path::new(file_path.as_str())).unwrap();

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
}
