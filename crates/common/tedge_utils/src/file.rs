use nix::unistd::*;
use std::fs::File;
use std::os::linux::fs::MetadataExt;
use std::os::unix::fs::PermissionsExt;
use std::{fs, io};
use users::{get_group_by_name, get_user_by_name};

pub fn create_directory_with_user_group(
    grp_user: &str,
    dirs: Vec<&str>,
) -> Result<(), anyhow::Error> {
    for directory in dirs {
        match fs::create_dir(directory) {
            Ok(_) => {
                change_owner_and_permission(directory, grp_user, 0o775)?;
            }

            Err(e) => {
                if e.kind() == io::ErrorKind::AlreadyExists {
                    return Ok(());
                } else {
                    eprintln!(
                        "failed to create the directory {} due to error {}",
                        directory, e
                    );
                    return Err(e.into());
                }
            }
        }
    }
    Ok(())
}

pub fn create_file_with_user_group(grp_user: &str, files: Vec<&str>) -> Result<(), anyhow::Error> {
    for file in files {
        match File::create(file) {
            Ok(_) => {
                change_owner_and_permission(file, grp_user, 0o644)?;
            }
            Err(e) => {
                if e.kind() == io::ErrorKind::AlreadyExists {
                    return Ok(());
                } else {
                    eprintln!("failed to create the file {} due to error {}", file, e);
                    return Err(e.into());
                }
            }
        }
    }
    Ok(())
}

fn change_owner_and_permission(file: &str, grp_user: &str, mode: u32) -> anyhow::Result<()> {
    let ud = match get_user_by_name(grp_user) {
        Some(user) => user.uid(),
        None => {
            anyhow::bail!("user not found");
        }
    };

    let gd = match get_group_by_name(grp_user) {
        Some(group) => group.gid(),
        None => {
            anyhow::bail!("group not found");
        }
    };

    let uid = fs::metadata(file)?.st_uid();
    let gid = fs::metadata(file)?.st_gid();

    // if user and group is same as existing, then do not change
    if (ud != uid) && (gd != gid) {
        chown(
            file,
            Some(Uid::from_raw(ud.into())),
            Some(Gid::from_raw(gd.into())),
        )?;
    }

    let mut perm = fs::metadata(file)?.permissions();
    perm.set_mode(mode);
    fs::set_permissions(file, perm)?;

    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    #[test]
    fn create_file() {
        let user = whoami::username();
        let _ = create_file_with_user_group(&user, vec!["/tmp/fcreate_test"]).unwrap();
        assert!(Path::new("/tmp/fcreate_test").exists());
        let meta = std::fs::metadata("/tmp/fcreate_test").unwrap();
        let perm = meta.permissions();
        println!("{:o}", perm.mode());
        assert!(format!("{:o}", perm.mode()).contains("644"));
    }

    #[test]
    fn create_directory() {
        let user = whoami::username();
        let _ = create_directory_with_user_group(&user, vec!["/tmp/fcreate_test_dir"]).unwrap();
        assert!(Path::new("/tmp/fcreate_test_dir").exists());
        let meta = std::fs::metadata("/tmp/fcreate_test_dir").unwrap();
        let perm = meta.permissions();
        println!("{:o}", perm.mode());
        assert!(format!("{:o}", perm.mode()).contains("775"));
    }
}
