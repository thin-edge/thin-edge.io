use nix::fcntl::{flock, FlockArg};
use std::{
    fs::{self, File, OpenOptions},
    io,
    os::unix::io::AsRawFd,
    path::{Path, PathBuf},
};
use tracing::{debug, error, info, warn};

const LOCK_CHILD_DIRECTORY: &str = "lock/";

#[derive(thiserror::Error, Debug)]
pub enum FlockfileError {
    #[error("Couldn't acquire file lock.")]
    FromIo {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Couldn't acquire file lock.")]
    FromNix {
        path: PathBuf,
        #[source]
        source: nix::Error,
    },
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
    pub fn new_lock(path: impl AsRef<Path>) -> Result<Flockfile, FlockfileError> {
        let path = PathBuf::new().join(path);
        let file = match OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&path)
        {
            Ok(file) => file,
            Err(err) => {
                return Err(FlockfileError::FromIo { path, source: err });
            }
        };

        let () = match flock(file.as_raw_fd(), FlockArg::LockExclusiveNonblock) {
            Ok(()) => (),
            Err(err) => {
                return Err(FlockfileError::FromNix { path, source: err });
            }
        };

        info!(r#"Lockfile created {:?}"#, &path);
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
    /// The Drop trait will be called always when the lock goes out of scope, however,
    /// if the program exits unexpectedly and drop is not called the lock will be removed by the system.
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            drop(handle);

            // Even if the file is not removed this is not an issue, as OS will take care of the flock.
            // Additionally if the file is created before an attempt to create the lock that won't be an issue as we rely on filesystem lock.
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

/// Check `run_dir`/lock/ for a lock file of a given `app_name`
pub fn check_another_instance_is_not_running(
    app_name: &str,
    run_dir: &Path,
) -> Result<Flockfile, FlockfileError> {
    match Flockfile::new_lock(
        run_dir
            .join(format!("{}{}.lock", LOCK_CHILD_DIRECTORY, app_name))
            .as_path(),
    ) {
        Ok(file) => Ok(file),
        Err(err) => {
            return match &err {
                FlockfileError::FromIo { path, .. } | FlockfileError::FromNix { path, .. } => {
                    error!("Another instance of {} is running.", app_name);
                    error!("Lock file path: {}", path.as_path().to_str().unwrap());
                    Err(err)
                }
            }
        }
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
    fn lock_out_of_scope() {
        let path = NamedTempFile::new().unwrap().into_temp_path().to_owned();
        {
            let _lockfile = Flockfile::new_lock(&path).unwrap();
            // assert!(path.exists());
            assert!(fs::metadata(&path).is_ok());
        }

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
            FlockfileError::FromNix { .. }
        );
    }
}
