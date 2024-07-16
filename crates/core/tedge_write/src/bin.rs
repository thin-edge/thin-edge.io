// TODO: force `destination_path` to be the first argument in clap

use std::fs;
use std::io;
use std::os::unix::fs::chown;
use std::os::unix::fs::MetadataExt;
use std::os::unix::fs::PermissionsExt;

use anyhow::bail;
use anyhow::Context;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use clap::Parser;

/// A binary used for writing to files which `tedge` user does not have write permissions for, using
/// sudo.
#[derive(Debug, Clone, PartialEq, Eq, Parser)]
#[command(about, version, long_about = None)]
pub struct Args {
    /// A canonical path to a file which will be written to.
    ///
    /// If the file does not exist, it will be created with the specified owner/group/permissions.
    /// If the file does exist, it will be overwritten, but its owner/group/permissions will remain
    /// unchanged.
    destination_path: Utf8PathBuf,

    /// Permission mode for the file, in octal form.
    #[arg(long)]
    mode: Option<Box<str>>,

    /// User which will become the new owner of the file.
    #[arg(long)]
    user: Option<Box<str>>,

    /// Group which will become the new owner of the file.
    #[arg(long)]
    group: Option<Box<str>>,
}

pub fn run(args: Args) -> anyhow::Result<()> {
    // /etc/sudoers can contain rules where sudo permissions are given to `tedge` user depending on
    // the files we write to, e.g:
    //
    // `tedge    ALL = (ALL) NOPASSWD: /usr/bin/tedge-write /etc/*`
    //
    // If the destination path contains `..` then we can "escape" outside the directory we're
    // allowed to write to. For that reason, we require paths to be canonical.
    //
    // Ideally this would be solved by a more expressive filesystem permissions system, e.g. ACL,
    // but unfortunately they're not standard on Linux, so we're stuck with trying to do next best
    // thing with sudo.
    if !args.destination_path.is_absolute() {
        bail!("Destination path has to be absolute");
    }

    let target_filepath: Utf8PathBuf = path_clean::clean(args.destination_path.as_str()).into();

    if target_filepath != *args.destination_path {
        bail!(
            "Destination path {} is not canonical",
            args.destination_path
        );
    }

    let file_existed_before_write = target_filepath.is_file();

    write_stdin_to_file_atomic(&target_filepath)?;

    if file_existed_before_write {
        return Ok(());
    }

    if args.user.is_some() || args.group.is_some() {
        chown_by_user_and_group_name(
            &target_filepath,
            args.user.as_deref(),
            args.group.as_deref(),
        )
        .context("Changing ownership of destination file failed")?;
    }

    if let Some(mode) = args.mode {
        let mode = u32::from_str_radix(&mode, 8).context("Parsing mode failed")?;
        let permissions = fs::Permissions::from_mode(mode);
        fs::set_permissions(args.destination_path.as_std_path(), permissions)
            .context("Could not set new permissions")?;
    }

    Ok(())
}

/// Writes contents of stdin into a file atomically.
///
/// To write a file atomically, stdin is written into a temporary file, which is then renamed into the target file.
/// Because rename is only atomic if both source and destination are on the same filesystem, the temporary file is
/// located in the same directory as the target file.
///
/// Using [`io::copy`], data should be copied using Linux-specific syscalls for copying between file descriptors,
/// without unnecessary copying to and from a userspace buffer.
fn write_stdin_to_file_atomic(target_filepath: &Utf8Path) -> anyhow::Result<()> {
    let temp_filepath = {
        let Some(temp_filename) = target_filepath.file_name().map(|f| format!("{f}.tmp")) else {
            bail!("Destination path {target_filepath} does not name a valid filename");
        };
        target_filepath.with_file_name(temp_filename)
    };

    // can fail if no permissions or temporary file already exists
    let mut temp_file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temp_filepath.as_std_path())
        .with_context(|| format!("Could not open temporary file `{temp_filepath}` for writing"))?;

    // If the target file already exists, use the same permissions and uid/gid
    if target_filepath.is_file() {
        let target_metadata = target_filepath.metadata().with_context(|| {
            format!("Could not fetch metadata of target file {target_filepath}")
        })?;

        let uid = target_metadata.uid();
        let gid = target_metadata.gid();
        chown(&temp_filepath, Some(uid), Some(gid))
            .context("Could not set destination owner/group")?;

        let target_permissions = target_metadata.permissions();
        temp_file
            .set_permissions(target_permissions)
            .with_context(|| {
                format!("Could not set permissions for temporary file {temp_filepath}")
            })?;
    }

    let mut stdin = std::io::stdin().lock();
    io::copy(&mut stdin, &mut temp_file)
        .with_context(|| format!("Could not write to the temporary file `{temp_filepath}`"))?;

    if let Err(e) = fs::rename(&temp_filepath, target_filepath)
        .with_context(|| format!("Could not write to destination file `{target_filepath}`"))
    {
        let _ = fs::remove_file(&temp_filepath);
        return Err(e);
    }
    Ok(())
}

fn chown_by_user_and_group_name(
    filepath: &Utf8Path,
    user: Option<&str>,
    group: Option<&str>,
) -> anyhow::Result<()> {
    // if user and group contain only digits, then they're ids
    let user_id = user.and_then(|u| u.parse::<u32>().ok());
    let group_id = group.and_then(|g| g.parse::<u32>().ok());

    let new_uid = match user {
        Some(u) => {
            if user_id.is_some() {
                user_id
            } else {
                Some(
                    uzers::get_user_by_name(u)
                        .with_context(|| format!("User `{u}` does not exist"))?
                        .uid(),
                )
            }
        }
        None => None,
    };

    let new_gid = match group {
        Some(g) => {
            if group_id.is_some() {
                group_id
            } else {
                Some(
                    uzers::get_group_by_name(g)
                        .with_context(|| format!("Group `{g}` does not exist"))?
                        .gid(),
                )
            }
        }
        None => None,
    };

    nix::unistd::chown(
        filepath.as_std_path(),
        new_uid.map(nix::unistd::Uid::from_raw),
        new_gid.map(nix::unistd::Gid::from_raw),
    )
    .with_context(|| format!("chown failed for file `{filepath}`"))?;

    Ok(())
}
