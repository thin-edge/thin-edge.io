use nix::unistd::*;
use std::fs::File;
use std::os::linux::fs::MetadataExt;
use std::os::unix::fs::PermissionsExt;
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
}

pub fn create_directory_with_user_group(
    dir: &str,
    user: &str,
    group: &str,
    mode: u32,
) -> Result<(), FileError> {
    match fs::create_dir(dir) {
        Ok(_) => {
            change_owner_and_permission(dir, user, group, mode)?;
        }

        Err(e) => {
            if e.kind() == io::ErrorKind::AlreadyExists {
                return Ok(());
            } else {
                return Err(FileError::DirectoryCreateFailed {
                    dir: dir.to_string(),
                    from: e,
                });
            }
        }
    }
    Ok(())
}

pub fn create_file_with_user_group(
    file: &str,
    user: &str,
    group: &str,
    mode: u32,
) -> Result<(), FileError> {
    match File::create(file) {
        Ok(_) => {
            change_owner_and_permission(file, user, group, mode)?;
        }
        Err(e) => {
            if e.kind() == io::ErrorKind::AlreadyExists {
                return Ok(());
            } else {
                return Err(FileError::FileCreateFailed {
                    file: file.to_string(),
                    from: e,
                });
            }
        }
    }
    Ok(())
}

fn change_owner_and_permission(
    file: &str,
    user: &str,
    group: &str,
    mode: u32,
) -> Result<(), FileError> {
    let ud = match get_user_by_name(user) {
        Some(user) => user.uid(),
        None => {
            return Err(FileError::UserNotFound { user: user.into() });
        }
    };

    let gd = match get_group_by_name(group) {
        Some(group) => group.gid(),
        None => {
            return Err(FileError::GroupNotFound {
                group: group.into(),
            });
        }
    };

    let uid = fs::metadata(file)
        .map_err(|e| FileError::MetaDataError {
            name: file.to_string(),
            from: e,
        })?
        .st_uid();
    let gid = fs::metadata(file)
        .map_err(|e| FileError::MetaDataError {
            name: file.to_string(),
            from: e,
        })?
        .st_gid();

    // if user and group is same as existing, then do not change
    if (ud != uid) && (gd != gid) {
        chown(
            file,
            Some(Uid::from_raw(ud.into())),
            Some(Gid::from_raw(gd.into())),
        )?;
    }

    let mut perm = fs::metadata(file)
        .map_err(|e| FileError::MetaDataError {
            name: file.to_string(),
            from: e,
        })?
        .permissions();
    perm.set_mode(mode);

    fs::set_permissions(file, perm).map_err(|e| FileError::MetaDataError {
        name: file.to_string(),
        from: e,
    })?;

    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    #[test]
    fn create_file_correct_user_group() {
        let user = whoami::username();
        let _ = create_file_with_user_group("/tmp/fcreate_test", &user, &user, 0o644).unwrap();
        assert!(Path::new("/tmp/fcreate_test").exists());
        let meta = std::fs::metadata("/tmp/fcreate_test").unwrap();
        let perm = meta.permissions();
        println!("{:o}", perm.mode());
        assert!(format!("{:o}", perm.mode()).contains("644"));
        fs::remove_file("/tmp/fcreate_test").unwrap();
    }

    #[test]
    fn create_file_wrong_user() {
        let user = whoami::username();
        let err = create_file_with_user_group("/tmp/fcreate_wrong_user", "test", &user, 0o775)
            .unwrap_err();

        assert!(err.to_string().contains("User not found"));
        fs::remove_file("/tmp/fcreate_wrong_user").unwrap();
    }

    #[test]
    fn create_file_wrong_group() {
        let user = whoami::username();
        let err = create_file_with_user_group("/tmp/fcreate_wrong_group", &user, "test", 0o775)
            .unwrap_err();

        assert!(err.to_string().contains("Group not found"));
        fs::remove_file("/tmp/fcreate_wrong_group").unwrap();
    }

    #[test]
    fn create_directory_with_correct_user_group() {
        let user = whoami::username();
        let _ =
            create_directory_with_user_group("/tmp/fcreate_test_dir", &user, &user, 0o775).unwrap();

        assert!(Path::new("/tmp/fcreate_test_dir").exists());
        let meta = std::fs::metadata("/tmp/fcreate_test_dir").unwrap();
        let perm = meta.permissions();
        println!("{:o}", perm.mode());
        assert!(format!("{:o}", perm.mode()).contains("775"));
        fs::remove_dir("/tmp/fcreate_test_dir").unwrap();
    }

    #[test]
    fn create_directory_with_wrong_user() {
        let user = whoami::username();

        let err = create_directory_with_user_group("/tmp/wrong_user_dir", "test", &user, 0o775)
            .unwrap_err();

        assert!(err.to_string().contains("User not found"));
        fs::remove_dir("/tmp/wrong_user_dir").unwrap();
    }

    #[test]
    fn create_directory_with_wrong_group() {
        let user = whoami::username();

        let err = create_directory_with_user_group("/tmp/wrong_group_dir", &user, "test", 0o775)
            .unwrap_err();

        assert!(err.to_string().contains("Group not found"));
        fs::remove_dir("/tmp/wrong_group_dir").unwrap();
    }
}
