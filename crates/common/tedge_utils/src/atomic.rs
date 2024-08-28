//! Utilities for atomic writes.
//!
//! For deployment of configuration files, we need to create a file atomically because certain
//! programs might watch configuration file for changes, so if it's not written atomically, then
//! file might be only partially written and a program trying to read it may crash.
//!
//! Atomic write of a file consists of creating a temporary file in the same directory, filling it
//! with correct content and permissions, and only then renaming the temporary into the destination
//! filename. Because we're never actually writing into the file, we don't need to write permissions
//! for the destination file, even if it exists. Instead we need only write/execute permissions to
//! the directory file is located in unless the directory has a sticky bit set. Overwriting a file
//! will also change its uid/gid/mode, if writing process euid/egid is different from file's
//! uid/gid. To keep uid/gid the same, after the write we need to do `chown`, and to do it we need
//! sudo.

use std::io::ErrorKind;
use std::io::Read;
use std::os::unix::fs::fchown;
use std::os::unix::fs::MetadataExt;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use anyhow::Context;

/// Writes a file atomically and optionally sets its permissions.
///
/// Setting ownership of a file is a privileged operation so it needs to be run as root. If any of
/// the filesystem operations fail due to not having permissions, the function will return an error.
///
/// If the file already exists, its content will be overwritten but its permissions will remain
/// unchanged.
pub fn write_file_atomic_set_permissions_if_doesnt_exist(
    mut src: impl Read,
    dest: impl AsRef<Path>,
    permissions: &MaybePermissions,
) -> anyhow::Result<()> {
    let dest = dest.as_ref();

    let target_permissions = target_permissions(dest, permissions)
        .context("failed to compute target permissions of the file")?;

    // TODO: create tests to ensure writes we expect are atomic
    let mut tempfile = tempfile::Builder::new()
        .permissions(std::fs::Permissions::from_mode(0o600))
        .tempfile_in(dest.parent().context("invalid path")?)
        .with_context(|| {
            format!(
                "could not create temporary file at '{}'",
                dest.to_string_lossy()
            )
        })?;

    std::io::copy(&mut src, &mut tempfile).context("failed to copy")?;

    tempfile
        .as_file()
        .set_permissions(std::fs::Permissions::from_mode(target_permissions.mode))
        .context("failed to set mode on the destination file")?;

    fchown(
        tempfile.as_file(),
        Some(target_permissions.uid),
        Some(target_permissions.gid),
    )
    .context("failed to change ownership of the destination file")?;

    tempfile.as_file().sync_all()?;

    tempfile
        .persist(dest)
        .context("failed to persist temporary file at destination")?;

    Ok(())
}

/// Computes target permissions for the file.
///
/// - if file exists preserve current permissions
/// - if it doesn't exist apply permissions from `permissions` if they are defined
/// - set to root:root with default umask otherwise
///
/// # Errors
/// - if desired user/group doesn't exist on the system
/// - no permission to read destination file
fn target_permissions(dest: &Path, permissions: &MaybePermissions) -> anyhow::Result<Permissions> {
    let current_file_permissions = match std::fs::metadata(dest) {
        Err(err) => match err.kind() {
            ErrorKind::NotFound => None,
            _ => return Err(err.into()),
        },
        Ok(p) => Some(p),
    };

    let uid = current_file_permissions
        .as_ref()
        .map(|p| p.uid())
        .or(permissions.uid)
        .unwrap_or(0);

    let gid = current_file_permissions
        .as_ref()
        .map(|p| p.gid())
        .or(permissions.gid)
        .unwrap_or(0);

    let mode = current_file_permissions
        .as_ref()
        .map(|p| p.mode())
        .or(permissions.mode)
        .unwrap_or(0o644);

    Ok(Permissions { uid, gid, mode })
}

pub struct MaybePermissions {
    pub uid: Option<u32>,
    pub gid: Option<u32>,
    pub mode: Option<u32>,
}

struct Permissions {
    uid: u32,
    gid: u32,
    mode: u32,
}
