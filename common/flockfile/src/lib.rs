use log::{debug, warn};
use nix::fcntl::{flock, FlockArg};
use std::{
    fs::{self, File, OpenOptions},
    io,
    os::unix::io::AsRawFd,
    path::{Path, PathBuf},
};

#[derive(thiserror::Error, Debug)]
pub enum FlockfileError {
    #[error(transparent)]
    IoError(#[from] std::io::Error),

    #[error("Couldn't acquire file lock.")]
    NixError(#[from] nix::Error),
}

/// flockfile creates a lockfile in the filesystem under `/run/lock` and then creates a filelock using system fcntl with flock.
/// flockfile will automatically remove lockfile on application exit and the OS should cleanup the filelock afterwards.
/// If application exits unexpectedly the filelock will be dropped, but the lockfile will not be removed unless handled in signal handler.
#[derive(Debug)]
pub struct Flockfile {
    handle: Option<File>,
    pub path: PathBuf,
}

impl Flockfile {
    /// Create new lockfile in `/run/lock` with specific name:
    ///
    /// #Example
    ///
    /// let _lockfile = match flockfile::Flockfile::new_lock("app")).unwrap();
    ///
    pub fn new_lock(lock_name: impl AsRef<Path>) -> Result<Flockfile, FlockfileError> {
        let path = Path::new("/run/lock").join(lock_name);

        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&path)?;

        if let Err(err) = flock(file.as_raw_fd(), FlockArg::LockExclusiveNonblock) {
            return Err(err.into());
        }

        Ok(Flockfile {
            handle: Some(file),
            path,
        })
    }

    /// Manually remove filelock and lockfile from the filesystem, this method doesn't have to be called explicitly,
    /// however if access to the locked file is required this must be called.
    pub fn unlock(mut self) -> Result<(), io::Error> {
        self.handle.take().expect("handle dropped");
        fs::remove_file(&self.path)?;
        Ok(())
    }
}

impl Drop for Flockfile {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            drop(handle);

            match fs::remove_file(&self.path) {
                Ok(()) => debug!(r#"Lockfile deleted "{:?}""#, self.path),
                Err(err) => warn!(
                    r#"Error while handling lockfile at "{:?}": {:?}"#,
                    self.path, err
                ),
            }
        }
    }
}

impl AsRef<Path> for Flockfile {
    fn as_ref(&self) -> &Path {
        self.path.as_ref()
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use assert_matches::*;
    use std::{fs, io};
    use tempfile::NamedTempFile;

    #[test]
    fn lock_access_remove() {
        let path = NamedTempFile::new().unwrap().into_temp_path().to_owned();
        let lockfile = Flockfile::new_lock(&path).unwrap();

        assert_eq!(lockfile.path, path);

        lockfile.unlock().unwrap();

        assert_eq!(
            fs::metadata(path).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
    }

    #[test]
    fn lock_twice() {
        let path = NamedTempFile::new().unwrap().into_temp_path().to_owned();
        let _lockfile = Flockfile::new_lock(&path).unwrap();

        assert_matches!(
            Flockfile::new_lock(&path).unwrap_err(),
            FlockfileError::NixError(_)
        );
    }
}
