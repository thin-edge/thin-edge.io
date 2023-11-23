//! A binary used for writing to files which `tedge` user does not have write permissions for, using
//! sudo.
//!
//! https://github.com/thin-edge/thin-edge.io/issues/2456

// TODO: force `destination_path` to be the first argument in clap

use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;

use anyhow::bail;
use anyhow::Context;
use camino::Utf8Path;
use clap::Parser;

/// A binary used for writing to files which `tedge` user does not have write permissions for, using
/// sudo.
#[derive(Debug, Clone, PartialEq, Eq, Parser)]
#[command(about, version, long_about = None)]
pub struct Args {
    /// A canonical path to a file which will be written to.
    ///
    /// If the file does not exist, it will be created. If the file does exist and is not empty, the
    /// file will be overridden.
    destination_path: Box<Utf8Path>,

    /// Permission mode for the file, in octal form.
    #[arg(long)]
    mode: Option<u32>,

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
    let target_filepath = args
        .destination_path
        .canonicalize_utf8()
        .context("Destination path is not utf-8")?;

    if target_filepath != *args.destination_path {
        bail!(
            "Destination path {} is not canonical",
            args.destination_path
        );
    }

    write_stdin_to_file_atomic(&target_filepath)?;

    if args.user.is_some() || args.group.is_some() {
        chown_by_user_and_group_name(
            &target_filepath,
            args.user.as_deref(),
            args.group.as_deref(),
        )
        .context("Changing ownership of destination file failed")?;
    }

    if let Some(mode) = args.mode {
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

    let mut temp_file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temp_filepath.as_std_path())
        .with_context(|| {
            format!(
                "Could not open temporary file `{}` for writing",
                temp_filepath
            )
        })?;

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
    user_name: Option<&str>,
    group_name: Option<&str>,
) -> anyhow::Result<()> {
    let new_uid = match user_name {
        Some(u) => Some(
            uzers::get_user_by_name(u)
                .with_context(|| format!("User `{u}` does not exist"))?
                .uid(),
        ),
        None => None,
    };

    let new_gid = match group_name {
        Some(g) => Some(
            uzers::get_group_by_name(g)
                .with_context(|| format!("Group `{g}` does not exist"))?
                .gid(),
        ),
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
