use nix::fcntl::flock;
use nix::fcntl::FlockArg;
use nix::unistd::write;
use std::fs::File;
use std::fs::OpenOptions;
use std::fs::{self};
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::io::AsRawFd;
use std::path::Path;
use std::path::PathBuf;
use tracing::debug;
use tracing::error;
use tracing::info;
use tracing::warn;

const LOCK_CHILD_DIRECTORY: &str = "lock/";

#[derive(thiserror::Error, Debug)]
pub enum FlockfileError {
    #[error("Couldn't create file lock.")]
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

impl FlockfileError {
    fn path(&self) -> &Path {
        match self {
            FlockfileError::FromIo { path, .. } => path,
            FlockfileError::FromNix { path, .. } => path,
        }
    }

    /// Is the error due to concurrent accesses on the lock file?
    /// Or to some unrelated issue as a permission denied error opening the file?
    fn non_exclusive_access(&self) -> bool {
        match self {
            FlockfileError::FromIo { .. } => false,
            FlockfileError::FromNix { .. } => true,
        }
    }
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
    /// Create an exclusive lock file with the given path (usually in in `/run/lock`)
    pub fn new_lock(path: impl AsRef<Path>) -> Result<Flockfile, FlockfileError> {
        // Ensure the lock file exists, ignoring errors at this stage.
        // This lock file is made world writable so different users can acquire in turn the lock;
        // even when the lock file has not been properly remove by a defunct process.
        // This doesn't prevent effective locking as this is ensured by the `flock` system call
        // and not by the existence of the lock file.
        let path = PathBuf::new().join(path);
        let _ = Flockfile::create_world_writable_lock_file(&path);

        // Open the lock file __without__ the `O_CREAT` option
        // that would prevent on recent Unix systems a user, that is not the owner of the file,
        // to open it with write access despite rw access.
        // see https://github.com/thin-edge/thin-edge.io/issues/1726
        let file =
            OpenOptions::new()
                .write(true)
                .open(&path)
                .map_err(|err| FlockfileError::FromIo {
                    path: path.clone(),
                    source: err,
                })?;

        // Convert the PID to a string
        let pid_string = format!("{}", std::process::id());

        flock(file.as_raw_fd(), FlockArg::LockExclusiveNonblock).map_err(|err| {
            FlockfileError::FromNix {
                path: path.clone(),
                source: err,
            }
        })?;

        // Write the PID to the lock file
        write(file.as_raw_fd(), pid_string.as_bytes()).map_err(|err| FlockfileError::FromNix {
            path: path.clone(),
            source: err,
        })?;

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

    /// Create the lock file if it does not exist,
    /// making sure any user can use this file as a lock.
    fn create_world_writable_lock_file(path: &PathBuf) -> Result<(), io::Error> {
        let file = File::create(path)?;
        let permissions = fs::Permissions::from_mode(0o666);

        file.set_permissions(permissions)
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
    let lock_path = run_dir.join(format!("{}{}.lock", LOCK_CHILD_DIRECTORY, app_name));

    Flockfile::new_lock(lock_path.as_path()).map_err(|err| {
        if err.non_exclusive_access() {
            error!("Another instance of {} is running.", app_name);
        }
        error!("Lock file path: {}", err.path().to_str().unwrap());
        err
    })
}

#[cfg(test)]
mod tests {

    use super::*;
    use assert_matches::*;
    use nix::unistd::Pid;
    use std::fs;
    use std::io;
    use std::io::Read;
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

    #[test]
    fn check_pid() {
        let path = NamedTempFile::new().unwrap().into_temp_path().to_owned();
        let _lockfile = Flockfile::new_lock(&path).unwrap();

        let mut read_lockfile = OpenOptions::new().read(true).open(&path).unwrap();

        let mut pid_string = String::new();
        read_lockfile.read_to_string(&mut pid_string).unwrap();

        let pid = Pid::from_raw(pid_string.parse().unwrap());

        assert_eq!(pid, Pid::this());
    }
}
