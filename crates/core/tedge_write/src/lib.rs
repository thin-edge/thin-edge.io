//! A binary used for writing to files which `tedge` user does not have write permissions for, using sudo.
//!
//! https://github.com/thin-edge/thin-edge.io/issues/2456

use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;

use anyhow::Context;
use camino::Utf8Path;
use clap::Parser;

#[derive(Debug, Clone, PartialEq, Eq, Parser)]
#[command(about, version, long_about = None)]
pub struct Args {
    /// A destination file which will be written to. Current content of the file will be lost.
    destination_file: Box<Utf8Path>,

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
    let mut stdin = std::io::stdin().lock();

    let mut destination = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(args.destination_file.as_std_path())
        .with_context(|| {
            format!(
                "Could not open destination file `{}` for writing",
                args.destination_file
            )
        })?;

    io::copy(&mut stdin, &mut destination).context("Could not copy source file to destination")?;

    if args.user.is_some() || args.group.is_some() {
        let new_uid = match args.user {
            Some(u) => Some(
                uzers::get_user_by_name(u.as_ref())
                    .with_context(|| format!("Invalid user: `{u}`"))?
                    .uid(),
            ),
            None => None,
        };

        let new_gid = match args.group {
            Some(g) => Some(
                uzers::get_group_by_name(g.as_ref())
                    .with_context(|| format!("Invalid group: `{g}`"))?
                    .gid(),
            ),
            None => None,
        };

        nix::unistd::chown(
            args.destination_file.as_std_path(),
            new_uid.map(nix::unistd::Uid::from_raw),
            new_gid.map(nix::unistd::Gid::from_raw),
        )
        .context("Could not set new permissions")?;
    }

    if let Some(mode) = args.mode {
        let permissions = fs::Permissions::from_mode(mode);
        fs::set_permissions(args.destination_file.as_std_path(), permissions)
            .context("Could not set new permissions")?;
    }

    Ok(())
}
