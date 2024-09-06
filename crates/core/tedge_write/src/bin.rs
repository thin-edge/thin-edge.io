// TODO: force `destination_path` to be the first argument in clap

use anyhow::bail;
use anyhow::Context;
use camino::Utf8PathBuf;
use clap::Parser;
use tedge_utils::atomic::MaybePermissions;

/// tee-like helper for writing to files which `tedge` user does not have write permissions to.
///
/// To be used in combination with sudo, passing the file content via standard input.
#[derive(Debug, Clone, PartialEq, Eq, Parser)]
#[command(about, version, long_about)]
pub struct Args {
    /// A canonical path to a file to which standard input will be written.
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

    let mode = args
        .mode
        .map(|m| u32::from_str_radix(&m, 8).with_context(|| format!("invalid mode: {m}")))
        .transpose()?;

    let uid = args
        .user
        .map(|u| uzers::get_user_by_name(&*u).with_context(|| format!("no such user: '{u}'")))
        .transpose()?
        .map(|u| u.uid());

    let gid = args
        .group
        .map(|g| uzers::get_group_by_name(&*g).with_context(|| format!("no such group: '{g}'")))
        .transpose()?
        .map(|g| g.gid());

    // what permissions we want to set if the file doesn't exist
    let permissions = MaybePermissions { uid, gid, mode };

    let src = std::io::stdin().lock();

    tedge_utils::atomic::write_file_atomic_set_permissions_if_doesnt_exist(
        src,
        &target_filepath,
        &permissions,
    )
    .with_context(|| format!("failed to write to destination file '{target_filepath}'"))?;

    Ok(())
}
