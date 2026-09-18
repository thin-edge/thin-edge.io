//! Run a command as a background job, whose outcome is collected by a later workflow step.
//!
//! The `shell_execute` workflow runs the command as a *background script*:
//! the workflow moves to the state collecting the outcome *before* the command is started,
//! and persists this state. This is what makes it safe for a command to restart
//! the tedge-agent or the device: on restart, the agent resumes the command in the collecting state,
//! and never runs the command a second time.
//!
//! The job and the collector are synchronized using a lock held by the job for its whole duration.
//! The collector waits for this lock, then reads the outcome stored by the job.
//! A job which died before storing its outcome is reported as interrupted,
//! which is a failure, even when the interruption is the expected outcome
//! of a command restarting the agent or the device.

use crate::ShellOutcome;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use nix::fcntl::Flock;
use nix::fcntl::FlockArg;
use serde::Deserialize;
use serde::Serialize;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::ErrorKind;
use std::io::Write;
use std::time::Duration;

/// The reason reported for a command interrupted before it completed
pub const INTERRUPTED_REASON: &str =
    "The command was interrupted before completion, most likely by a restart of tedge-agent or of the device";

/// The files shared by a background job and the collector of its outcome
pub struct Job {
    dir: Utf8PathBuf,
}

/// The outcome of a job, as stored on disk
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum JobOutcome {
    /// The command has been run
    Completed {
        result: String,
        exit_code: i32,
        timed_out_ms: Option<u64>,
    },

    /// The command could not be run
    LaunchError { reason: String },

    /// The command has not completed, nor reported any outcome
    Interrupted,
}

impl From<ShellOutcome> for JobOutcome {
    fn from(outcome: ShellOutcome) -> Self {
        JobOutcome::Completed {
            result: outcome.result,
            exit_code: outcome.exit_code,
            timed_out_ms: outcome.timed_out.map(|t| t.as_millis() as u64),
        }
    }
}

impl JobOutcome {
    /// The outcome to be reported, `Err` being a failure to run the command
    pub fn into_shell_outcome(self) -> Result<ShellOutcome, String> {
        match self {
            JobOutcome::Completed {
                result,
                exit_code,
                timed_out_ms,
            } => Ok(ShellOutcome {
                result,
                exit_code,
                timed_out: timed_out_ms.map(Duration::from_millis),
            }),
            JobOutcome::LaunchError { reason } => Err(reason),
            JobOutcome::Interrupted => Err(INTERRUPTED_REASON.to_string()),
        }
    }
}

impl Job {
    /// The job of the command with the given identifier, stored under `tmp_dir`
    pub fn new(tmp_dir: &Utf8Path, cmd_id: &str) -> std::io::Result<Self> {
        // The command id is used as a directory name
        let valid = !cmd_id.is_empty()
            && !cmd_id.starts_with('.')
            && cmd_id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':'));
        if !valid {
            return Err(std::io::Error::new(
                ErrorKind::InvalidInput,
                format!("Invalid command id: {cmd_id:?}"),
            ));
        }

        Ok(Job {
            dir: tmp_dir.join("tedge-shell-plugin").join(cmd_id),
        })
    }

    /// Run the job, storing its outcome for the collector
    ///
    /// The lock is held until this process exits, whatever the reason.
    pub fn run(
        &self,
        execute: impl FnOnce() -> Result<ShellOutcome, String>,
    ) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let lock = self.open_lock_file()?;
        let _lock = Flock::lock(lock, FlockArg::LockExclusiveNonblock).map_err(|(_, err)| {
            std::io::Error::other(format!("The command is already running: {err}"))
        })?;

        // A stale outcome would be taken as the outcome of this run, were it interrupted
        remove_if_exists(&self.outcome_path())?;

        let outcome = match execute() {
            Ok(outcome) => JobOutcome::from(outcome),
            Err(reason) => JobOutcome::LaunchError { reason },
        };
        self.store_outcome(&outcome)
    }

    /// Wait for the job to complete, and return its outcome
    ///
    /// The job files are removed once the outcome has been read.
    pub fn collect(&self) -> std::io::Result<JobOutcome> {
        let lock = match self.open_lock_file() {
            Ok(lock) => lock,
            // The job never started, or its files have been removed, e.g. by a device reboot
            Err(err) if matches!(err.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) => {
                let outcome = self.missing_outcome("The command could not be started");
                let _ = std::fs::remove_dir_all(&self.dir);
                return Ok(outcome);
            }
            Err(err) => return Err(err),
        };
        let lock = Flock::lock(lock, FlockArg::LockExclusive)
            .map_err(|(_, err)| std::io::Error::from(err))?;

        let outcome = match std::fs::read(self.outcome_path()) {
            Ok(content) => serde_json::from_slice(&content).map_err(std::io::Error::other)?,
            Err(err) if err.kind() == ErrorKind::NotFound => {
                self.missing_outcome("The command outcome could not be stored")
            }
            Err(err) => return Err(err),
        };

        drop(lock);
        std::fs::remove_dir_all(&self.dir)?;
        Ok(outcome)
    }

    /// The outcome of a job which stored none
    ///
    /// Either the job has been interrupted, or it failed to write its files,
    /// e.g. on a full disk or a tmp dir not writable by this user.
    /// As the job cannot report the latter, the collector tells them apart
    /// by writing to the job directory the same way the job does.
    fn missing_outcome(&self, context: &str) -> JobOutcome {
        match self.check_storage() {
            Ok(()) => JobOutcome::Interrupted,
            Err(err) => JobOutcome::LaunchError {
                reason: format!("{context}, as writing to '{}' failed: {err}", self.dir),
            },
        }
    }

    fn check_storage(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let mut file = tempfile::NamedTempFile::new_in(&self.dir)?;
        file.write_all(b"{}")?;
        file.as_file().sync_all()
    }

    fn open_lock_file(&self) -> std::io::Result<File> {
        OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(self.dir.join("lock"))
    }

    fn outcome_path(&self) -> Utf8PathBuf {
        self.dir.join("outcome.json")
    }

    /// Store the outcome atomically, so the collector never reads a partial outcome
    fn store_outcome(&self, outcome: &JobOutcome) -> std::io::Result<()> {
        let mut file = tempfile::NamedTempFile::new_in(&self.dir)?;
        file.write_all(&serde_json::to_vec(outcome).map_err(std::io::Error::other)?)?;
        file.as_file().sync_all()?;
        file.persist(self.outcome_path()).map_err(|err| err.error)?;
        Ok(())
    }
}

fn remove_if_exists(path: &Utf8Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(err) if err.kind() != ErrorKind::NotFound => Err(err),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    use tedge_test_utils::fs::TempTedgeDir;

    fn tmp_dir(ttd: &TempTedgeDir) -> &Utf8Path {
        ttd.path()
    }

    fn completed(result: &str) -> ShellOutcome {
        ShellOutcome {
            result: result.to_string(),
            exit_code: 0,
            timed_out: None,
        }
    }

    #[test]
    fn the_outcome_of_a_job_is_collected() {
        let ttd = TempTedgeDir::new();
        let job = Job::new(tmp_dir(&ttd), "c8y-mapper-1234").unwrap();

        job.run(|| Ok(completed("hello\n"))).unwrap();

        assert_eq!(
            job.collect().unwrap(),
            JobOutcome::Completed {
                result: "hello\n".to_string(),
                exit_code: 0,
                timed_out_ms: None
            }
        );
    }

    #[test]
    fn the_job_files_are_removed_once_collected() {
        let ttd = TempTedgeDir::new();
        let job = Job::new(tmp_dir(&ttd), "c8y-mapper-1234").unwrap();
        job.run(|| Ok(completed("hello\n"))).unwrap();

        job.collect().unwrap();

        assert!(!ttd
            .path()
            .join("tedge-shell-plugin/c8y-mapper-1234")
            .exists());
    }

    #[test]
    fn a_launch_error_is_collected() {
        let ttd = TempTedgeDir::new();
        let job = Job::new(tmp_dir(&ttd), "c8y-mapper-1234").unwrap();

        job.run(|| Err("no such shell".to_string())).unwrap();

        assert_eq!(
            job.collect().unwrap().into_shell_outcome(),
            Err("no such shell".to_string())
        );
    }

    #[test]
    fn a_timeout_is_collected() {
        let ttd = TempTedgeDir::new();
        let job = Job::new(tmp_dir(&ttd), "c8y-mapper-1234").unwrap();
        let outcome = ShellOutcome {
            result: "started\n".to_string(),
            exit_code: 124,
            timed_out: Some(Duration::from_secs(600)),
        };

        job.run(|| Ok(outcome)).unwrap();

        assert_eq!(
            job.collect().unwrap().into_shell_outcome(),
            Ok(ShellOutcome {
                result: "started\n".to_string(),
                exit_code: 124,
                timed_out: Some(Duration::from_secs(600)),
            })
        );
    }

    #[test]
    fn a_job_which_never_ran_is_reported_as_interrupted() {
        let ttd = TempTedgeDir::new();
        let job = Job::new(tmp_dir(&ttd), "c8y-mapper-1234").unwrap();

        assert_eq!(job.collect().unwrap(), JobOutcome::Interrupted);
    }

    #[test]
    fn a_job_which_died_without_outcome_is_reported_as_interrupted() {
        let ttd = TempTedgeDir::new();
        let job = Job::new(tmp_dir(&ttd), "c8y-mapper-1234").unwrap();
        // A job killed while running leaves its lock file, but no outcome
        std::fs::create_dir_all(ttd.path().join("tedge-shell-plugin/c8y-mapper-1234")).unwrap();
        job.open_lock_file().unwrap();

        assert_eq!(job.collect().unwrap(), JobOutcome::Interrupted);
    }

    #[test]
    fn a_job_which_could_not_write_its_files_is_reported_with_the_error() {
        let ttd = TempTedgeDir::new();
        let job = Job::new(tmp_dir(&ttd), "c8y-mapper-1234").unwrap();
        // A file where the job directory is expected makes any write fail, even as root
        ttd.file("tedge-shell-plugin");

        assert!(job.run(|| Ok(completed("hello\n"))).is_err());

        let JobOutcome::LaunchError { reason } = job.collect().unwrap() else {
            panic!("expected a launch error")
        };
        assert!(
            reason.starts_with("The command could not be started, as writing to '"),
            "{reason}"
        );
        assert!(
            reason.contains("tedge-shell-plugin/c8y-mapper-1234"),
            "{reason}"
        );
    }

    #[test]
    fn no_job_files_are_left_behind_when_reporting_an_interruption() {
        let ttd = TempTedgeDir::new();
        let job = Job::new(tmp_dir(&ttd), "c8y-mapper-1234").unwrap();

        assert_eq!(job.collect().unwrap(), JobOutcome::Interrupted);
        assert!(!ttd
            .path()
            .join("tedge-shell-plugin/c8y-mapper-1234")
            .exists());
    }

    #[test]
    fn an_interruption_is_reported_as_a_failure() {
        assert_eq!(
            JobOutcome::Interrupted.into_shell_outcome(),
            Err(INTERRUPTED_REASON.to_string())
        );
    }

    #[test]
    fn the_collector_waits_for_a_running_job() {
        let ttd = TempTedgeDir::new();
        let tmp = tmp_dir(&ttd).to_owned();

        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let job = std::thread::spawn(move || {
            Job::new(&tmp, "c8y-mapper-1234")
                .unwrap()
                .run(|| {
                    started_tx.send(()).unwrap();
                    std::thread::sleep(Duration::from_millis(500));
                    Ok(completed("done\n"))
                })
                .unwrap()
        });

        // The job holds the lock while executing the command
        started_rx.recv().unwrap();

        let started = Instant::now();
        let outcome = Job::new(tmp_dir(&ttd), "c8y-mapper-1234")
            .unwrap()
            .collect()
            .unwrap();
        job.join().unwrap();

        assert!(started.elapsed() >= Duration::from_millis(200));
        assert_eq!(
            outcome,
            JobOutcome::Completed {
                result: "done\n".to_string(),
                exit_code: 0,
                timed_out_ms: None
            }
        );
    }

    #[test]
    fn a_stale_outcome_is_not_taken_for_the_outcome_of_a_new_run() {
        let ttd = TempTedgeDir::new();
        let job = Job::new(tmp_dir(&ttd), "c8y-mapper-1234").unwrap();
        job.run(|| Ok(completed("stale\n"))).unwrap();

        // The new run dies before storing any outcome
        let result = std::panic::catch_unwind(|| {
            job.run(|| panic!("killed")).unwrap();
        });
        assert!(result.is_err());

        assert_eq!(job.collect().unwrap(), JobOutcome::Interrupted);
    }

    #[test]
    fn a_command_id_cannot_escape_the_job_directory() {
        let ttd = TempTedgeDir::new();
        for cmd_id in ["", ".", "..", "../foo", "foo/bar", "foo bar", ".hidden"] {
            assert!(
                Job::new(tmp_dir(&ttd), cmd_id).is_err(),
                "{cmd_id:?} should be rejected"
            );
        }
        for cmd_id in [
            "c8y-mapper-1234",
            "local-1234",
            "sub:shell_execute:1234",
            "a.b_c",
        ] {
            assert!(
                Job::new(tmp_dir(&ttd), cmd_id).is_ok(),
                "{cmd_id:?} should be accepted"
            );
        }
    }
}
