// TODO: force `destination_path` to be the first argument in clap

use anyhow::bail;
use anyhow::Context;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use clap::arg;
use clap::Parser;
use tedge_config::cli::CommonArgs;
use tedge_config::log_init;
use tedge_utils::atomic::MaybePermissions;
use tedge_utils::file::PermissionEntry;

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
    ///
    /// If parent directories are missing, they will be created and the specified parent permissions
    /// will be applied only to the immediate parent.
    /// If the parents exist, they will remain unchanged.
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

    /// Only create all parent directories if they are missing.
    #[arg(long)]
    create_dirs_only: bool,

    /// Permission mode for the immediate parent directory, in octal form.
    #[arg(long)]
    parent_mode: Option<Box<str>>,

    /// User which will become the new owner of the immediate parent directory.
    #[arg(long)]
    parent_user: Option<Box<str>>,

    /// Group which will become the new owner of the immediate parent directory.
    #[arg(long)]
    parent_group: Option<Box<str>>,

    #[command(flatten)]
    common: CommonArgs,
}

pub fn run(args: Args) -> anyhow::Result<()> {
    log_init(
        "tedge-write",
        &args.common.log_args,
        &args.common.config_dir,
    )?;

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

    // unwrap is safe because clean returns an utf8 path when given an utf8 path
    let target_path: Utf8PathBuf = path_clean::clean(args.destination_path.as_std_path())
        .try_into()
        .unwrap();

    if target_path != *args.destination_path {
        bail!(
            "Destination path {} is not canonical",
            args.destination_path
        );
    }

    // Create the parent directories if they are missing
    if args.create_dirs_only {
        if !target_path.exists() {
            create_parent_dirs(&args, &target_path)?;
        }
        return Ok(());
    }

    // what permissions we want to set if the file doesn't exist
    let file_permissions = get_permissions(args.mode, args.user, args.group)?;

    let src = std::io::stdin().lock();

    tedge_utils::atomic::write_file_atomic_set_permissions_if_doesnt_exist(
        src,
        &target_path,
        &file_permissions,
    )
    .with_context(|| format!("failed to write to destination file '{target_path}'"))?;

    Ok(())
}

fn create_parent_dirs(args: &Args, dir_path: &Utf8Path) -> anyhow::Result<()> {
    let parent_permissions = PermissionEntry::new(
        args.parent_user.clone().map(|s| s.into()),
        args.parent_group.clone().map(|s| s.into()),
        args.parent_mode
            .clone()
            .map(|m| u32::from_str_radix(&m, 8).with_context(|| format!("invalid mode: {m}")))
            .transpose()?,
    );

    match std::fs::create_dir_all(dir_path) {
        Ok(_) => {
            parent_permissions.apply_sync(dir_path.as_std_path())?;
        }
        Err(err) => {
            bail!("failed to create parent directories. path: '{dir_path}', error: '{err}'");
        }
    }

    Ok(())
}

fn get_permissions(
    mode: Option<Box<str>>,
    user: Option<Box<str>>,
    group: Option<Box<str>>,
) -> anyhow::Result<MaybePermissions> {
    let mode = mode
        .map(|m| u32::from_str_radix(&m, 8).with_context(|| format!("invalid mode: {m}")))
        .transpose()?;

    let uid = user
        .map(|u| uzers::get_user_by_name(&*u).with_context(|| format!("no such user: '{u}'")))
        .transpose()?
        .map(|u| u.uid());

    let gid = group
        .map(|g| uzers::get_group_by_name(&*g).with_context(|| format!("no such group: '{g}'")))
        .transpose()?
        .map(|g| g.gid());

    Ok(MaybePermissions { uid, gid, mode })
}
