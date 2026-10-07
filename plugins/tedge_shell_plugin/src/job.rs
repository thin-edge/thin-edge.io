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
//! which is a failure, unless the command declared the result to be reported in that case,
//! using [`Job::set_result`], e.g. before restarting the agent or the device.
//!
//! The output of the command is relayed to a file of the job as it is produced,
//! so the output of an interrupted command is reported, up to the interruption.
//!
//! The job files are stored in a directory expected to survive a device reboot.
//! Nothing is assumed though on the content of a file which might have been written
//! just before an abrupt reboot: an empty or truncated file is taken as a missing one.

use crate::OutputFile;
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
use std::time::Instant;
use tedge_utils::paths::ManagedDir;
use tedge_utils::paths::TedgePaths;

/// The reason reported for a command interrupted before it completed
pub const INTERRUPTED_REASON: &str =
    "The command was interrupted before completion, most likely by a restart of tedge-agent or of the device";

/// The notice added to the output of an interrupted command which declared its result
/// with [`Job::set_result`]
pub const INTERRUPTED_OUTPUT_NOTICE: &str =
    "<the command has been interrupted, its output may be incomplete>\n";

/// The reason reported for an interrupted command which declared a failure without giving a reason
pub const DECLARED_FAILURE_REASON: &str =
    "The command set its result as failed before being interrupted";

/// How long `set-result` waits for the output printed so far to be persisted
///
/// The job serves such a request within a fraction of a second,
/// so this is only reached when the job is gone.
#[cfg(not(test))]
const FLUSH_TIMEOUT: Duration = Duration::from_secs(5);

/// Shortened, so that the tests of a job run with no relay do not wait for nothing
#[cfg(test)]
const FLUSH_TIMEOUT: Duration = Duration::from_millis(100);

/// The result declared by a command, to be reported should it be interrupted
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "outcome")]
pub enum DeclaredResult {
    Successful,
    Failed { reason: Option<String> },
}

/// The files shared by a background job and the collector of its outcome
pub struct Job {
    dir: ManagedDir,
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

    /// The command has been interrupted, after declaring the result to be reported
    ResultSet {
        result: String,
        declared: DeclaredResult,
    },

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
                result_set: false,
                reason: None,
            }),
            JobOutcome::ResultSet { result, declared } => {
                let (exit_code, reason) = match declared {
                    DeclaredResult::Successful => (0, None),
                    DeclaredResult::Failed { reason } => (
                        1,
                        Some(reason.unwrap_or_else(|| DECLARED_FAILURE_REASON.to_string())),
                    ),
                };
                Ok(ShellOutcome {
                    result,
                    exit_code,
                    timed_out: None,
                    result_set: true,
                    reason,
                })
            }
            JobOutcome::LaunchError { reason } => Err(reason),
            JobOutcome::Interrupted => Err(INTERRUPTED_REASON.to_string()),
        }
    }
}

impl Job {
    /// The job of the command with the given identifier, stored under `data_dir`
    pub fn new(data_dir: &TedgePaths, cmd_id: &str) -> std::io::Result<Self> {
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

        let dir = data_dir
            .dir(Utf8Path::new("tedge-shell-plugin").join(cmd_id))
            .map_err(|err| std::io::Error::new(ErrorKind::InvalidInput, err))?;
        Ok(Job { dir })
    }

    /// Create the directory of the job files, owned as the other thin-edge directories
    ///
    /// To be called before the job is [run](Job::run).
    pub async fn create_dir(&self) -> std::io::Result<()> {
        self.dir.ensure().await.map_err(std::io::Error::other)
    }

    /// Run the job, storing its outcome for the collector
    ///
    /// The lock is held until this process exits, whatever the reason.
    /// The job directory is expected to have been created with [Job::create_dir].
    pub fn run(
        &self,
        execute: impl FnOnce(OutputFile) -> Result<ShellOutcome, String>,
    ) -> std::io::Result<()> {
        let lock = self.open_lock_file()?;
        let _lock = Flock::lock(lock, FlockArg::LockExclusiveNonblock).map_err(|(_, err)| {
            std::io::Error::other(format!("The command is already running: {err}"))
        })?;

        // A stale outcome would be taken as the outcome of this run, were it interrupted
        remove_if_exists(&self.outcome_path())?;
        remove_if_exists(&self.declared_result_path())?;
        remove_if_exists(&self.flush_request_path())?;
        // Read back once the command has completed
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(self.output_path())?;
        let output = OutputFile {
            file,
            flush_request: Some(self.flush_request_path()),
        };

        let outcome = match execute(output) {
            Ok(outcome) => JobOutcome::from(outcome),
            Err(reason) => JobOutcome::LaunchError { reason },
        };
        self.store_outcome(&outcome)?;
        // The output is part of the outcome, once stored
        remove_if_exists(&self.output_path())
    }

    /// Wait for the job to complete, and return its outcome
    ///
    /// At most `max_output_size` bytes of the output of an interrupted command are reported.
    /// The job files are removed once the outcome has been read.
    pub fn collect(&self, max_output_size: u32) -> std::io::Result<JobOutcome> {
        let lock = match self.open_lock_file() {
            Ok(lock) => lock,
            // The job never started, or its files have been removed, e.g. by a device reboot
            Err(err) if matches!(err.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) => {
                let outcome = self.missing_outcome("The command could not be started");
                let _ = std::fs::remove_dir_all(self.path());
                return Ok(outcome);
            }
            Err(err) => return Err(err),
        };
        let lock = Flock::lock(lock, FlockArg::LockExclusive)
            .map_err(|(_, err)| std::io::Error::from(err))?;

        let outcome = match read_json(&self.outcome_path())? {
            Some(outcome) => outcome,
            None => match read_json::<DeclaredResult>(&self.declared_result_path())? {
                Some(declared) => self.interrupted_with(declared, max_output_size)?,
                None => self.missing_outcome("The command outcome could not be stored"),
            },
        };

        drop(lock);
        std::fs::remove_dir_all(self.path())?;
        Ok(outcome)
    }

    /// Declare the result to be reported for the running job, should it be interrupted
    ///
    /// Called by the command itself, before it restarts the agent or the device,
    /// so it is reported with this result rather than as interrupted, if killed along with the agent.
    /// A command which completes anyway, e.g. because the restart failed,
    /// is reported with its actual outcome.
    pub fn set_result(&self, declared: &DeclaredResult) -> std::io::Result<()> {
        if !self.path().join("lock").exists() {
            return Err(std::io::Error::new(
                ErrorKind::NotFound,
                format!("No command is running with the files '{}'", self.path()),
            ));
        }

        // Persisted before returning, as the device might be rebooted right after,
        // along with the output printed so far
        self.flush_output()?;
        match File::open(self.output_path()) {
            Ok(output) => output.sync_all()?,
            Err(err) if err.kind() == ErrorKind::NotFound => (),
            Err(err) => return Err(err),
        }
        self.write_json(&self.declared_result_path(), declared)
    }

    /// Ask the job to persist the output relayed so far, and wait for it to be done
    ///
    /// The output printed by the command might still be in the pipe read by the job,
    /// which is drained before the output file is synced.
    /// Giving up after a while, e.g. if the job is gone, the output being then persisted as is.
    fn flush_output(&self) -> std::io::Result<()> {
        let request = self.flush_request_path();
        File::create(&request)?;
        let deadline = Instant::now() + FLUSH_TIMEOUT;
        while request.exists() {
            if Instant::now() >= deadline {
                tracing::warn!("The output printed so far might not be persisted, the command output not being relayed");
                return remove_if_exists(&request);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    }

    /// The outcome of an interrupted job, which declared its result
    ///
    /// The output of the command, up to the interruption, is reported,
    /// followed by a notice telling it may be incomplete.
    fn interrupted_with(
        &self,
        declared: DeclaredResult,
        max_output_size: u32,
    ) -> std::io::Result<JobOutcome> {
        let mut result = match File::open(self.output_path()) {
            Ok(mut output) => crate::read_output(&mut output, max_output_size)?,
            Err(err) if err.kind() == ErrorKind::NotFound => String::new(),
            Err(err) => return Err(err),
        };
        if !result.is_empty() && !result.ends_with('\n') {
            result.push('\n');
        }
        result.push_str(INTERRUPTED_OUTPUT_NOTICE);
        Ok(JobOutcome::ResultSet { result, declared })
    }

    /// The outcome of a job which stored none
    ///
    /// Either the job has been interrupted, or it failed to write its files,
    /// e.g. on a full disk or a data dir not writable by this user.
    /// As the job cannot report the latter, the collector tells them apart
    /// by writing to the job directory the same way the job does.
    fn missing_outcome(&self, context: &str) -> JobOutcome {
        match self.check_storage() {
            Ok(()) => JobOutcome::Interrupted,
            Err(err) => JobOutcome::LaunchError {
                reason: format!("{context}, as writing to '{}' failed: {err}", self.path()),
            },
        }
    }

    ///
    /// Nothing is created by the collector, which checks the closest existing directory
    /// on the path of the job directory instead, when the job has not created it.
    fn check_storage(&self) -> std::io::Result<()> {
        let dir = self
            .path()
            .ancestors()
            .find(|dir| dir.exists())
            .unwrap_or(self.path());
        let mut file = tempfile::NamedTempFile::new_in(dir)?;
        file.write_all(b"{}")?;
        file.as_file().sync_all()
    }

    fn path(&self) -> &Utf8Path {
        self.dir.path()
    }

    fn open_lock_file(&self) -> std::io::Result<File> {
        OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(self.path().join("lock"))
    }

    fn declared_result_path(&self) -> Utf8PathBuf {
        self.path().join("declared-result.json")
    }

    fn output_path(&self) -> Utf8PathBuf {
        self.path().join("output")
    }

    fn flush_request_path(&self) -> Utf8PathBuf {
        self.path().join("flush-request")
    }

    fn outcome_path(&self) -> Utf8PathBuf {
        self.path().join("outcome.json")
    }

    fn store_outcome(&self, outcome: &JobOutcome) -> std::io::Result<()> {
        self.write_json(&self.outcome_path(), outcome)
    }

    /// Store a value atomically, so the collector never reads a partial value
    fn write_json(&self, path: &Utf8Path, outcome: &impl Serialize) -> std::io::Result<()> {
        let mut file = tempfile::NamedTempFile::new_in(self.path())?;
        file.write_all(&serde_json::to_vec(outcome).map_err(std::io::Error::other)?)?;
        file.as_file().sync_all()?;
        file.persist(path).map_err(|err| err.error)?;
        Ok(())
    }
}

/// Read a value stored with [`Job::write_json`], if any
///
/// A file left empty or truncated by an abrupt reboot is taken as a missing one.
fn read_json<T: serde::de::DeserializeOwned>(path: &Utf8Path) -> std::io::Result<Option<T>> {
    match std::fs::read(path) {
        Ok(content) => match serde_json::from_slice(&content) {
            Ok(value) => Ok(Some(value)),
            Err(err) => {
                tracing::warn!("Ignoring the unreadable job file '{path}': {err}");
                Ok(None)
            }
        },
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
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

    const NO_LIMIT: u32 = u32::MAX;

    #[test]
    fn the_outcome_of_a_job_is_collected() {
        let ttd = TempTedgeDir::new();
        let job = created(Job::new(&data_dir(&ttd), "c8y-mapper-1234").unwrap());

        job.run(|_| Ok(completed("hello\n"))).unwrap();

        assert_eq!(
            job.collect(NO_LIMIT).unwrap(),
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
        let job = created(Job::new(&data_dir(&ttd), "c8y-mapper-1234").unwrap());
        job.run(|_| Ok(completed("hello\n"))).unwrap();

        job.collect(NO_LIMIT).unwrap();

        assert!(!ttd
            .path()
            .join("tedge-shell-plugin/c8y-mapper-1234")
            .exists());
    }

    #[test]
    fn a_launch_error_is_collected() {
        let ttd = TempTedgeDir::new();
        let job = created(Job::new(&data_dir(&ttd), "c8y-mapper-1234").unwrap());

        job.run(|_| Err("no such shell".to_string())).unwrap();

        assert_eq!(
            job.collect(NO_LIMIT).unwrap().into_shell_outcome(),
            Err("no such shell".to_string())
        );
    }

    #[test]
    fn a_timeout_is_collected() {
        let ttd = TempTedgeDir::new();
        let job = created(Job::new(&data_dir(&ttd), "c8y-mapper-1234").unwrap());
        let outcome = ShellOutcome {
            result: "started\n".to_string(),
            exit_code: 124,
            timed_out: Some(Duration::from_secs(600)),
            result_set: false,
            reason: None,
        };

        job.run(|_| Ok(outcome)).unwrap();

        assert_eq!(
            job.collect(NO_LIMIT).unwrap().into_shell_outcome(),
            Ok(ShellOutcome {
                result: "started\n".to_string(),
                exit_code: 124,
                timed_out: Some(Duration::from_secs(600)),
                result_set: false,
                reason: None,
            })
        );
    }

    #[test]
    fn a_job_which_never_ran_is_reported_as_interrupted() {
        let ttd = TempTedgeDir::new();
        let job = Job::new(&data_dir(&ttd), "c8y-mapper-1234").unwrap();

        assert_eq!(job.collect(NO_LIMIT).unwrap(), JobOutcome::Interrupted);
    }

    #[test]
    fn a_job_which_died_without_outcome_is_reported_as_interrupted() {
        let ttd = TempTedgeDir::new();
        let job = Job::new(&data_dir(&ttd), "c8y-mapper-1234").unwrap();
        // A job killed while running leaves its lock file, but no outcome
        ttd.dir("tedge-shell-plugin").dir("c8y-mapper-1234");
        job.open_lock_file().unwrap();

        assert_eq!(job.collect(NO_LIMIT).unwrap(), JobOutcome::Interrupted);
    }

    #[test]
    fn a_job_which_could_not_write_its_files_is_reported_with_the_error() {
        let ttd = TempTedgeDir::new();
        let job = Job::new(&data_dir(&ttd), "c8y-mapper-1234").unwrap();
        // A file where the job directory is expected makes any write fail, even as root
        ttd.file("tedge-shell-plugin");

        assert!(job.run(|_| Ok(completed("hello\n"))).is_err());

        let JobOutcome::LaunchError { reason } = job.collect(NO_LIMIT).unwrap() else {
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
        let job = Job::new(&data_dir(&ttd), "c8y-mapper-1234").unwrap();

        assert_eq!(job.collect(NO_LIMIT).unwrap(), JobOutcome::Interrupted);
        assert!(!ttd
            .path()
            .join("tedge-shell-plugin/c8y-mapper-1234")
            .exists());
    }

    #[test]
    fn a_declared_success_is_reported_for_an_interrupted_job() {
        let ttd = TempTedgeDir::new();
        let job = killed_job(&ttd);
        job.set_result(&DeclaredResult::Successful).unwrap();

        assert_eq!(
            job.collect(NO_LIMIT).unwrap().into_shell_outcome(),
            Ok(ShellOutcome {
                result: INTERRUPTED_OUTPUT_NOTICE.to_string(),
                exit_code: 0,
                timed_out: None,
                result_set: true,
                reason: None,
            })
        );
    }

    #[test]
    fn a_declared_failure_is_reported_for_an_interrupted_job() {
        let ttd = TempTedgeDir::new();
        let job = killed_job(&ttd);
        job.set_result(&DeclaredResult::Failed {
            reason: Some("Failed to do something".to_string()),
        })
        .unwrap();

        assert_eq!(
            job.collect(NO_LIMIT).unwrap().into_shell_outcome(),
            Ok(ShellOutcome {
                result: INTERRUPTED_OUTPUT_NOTICE.to_string(),
                exit_code: 1,
                timed_out: None,
                result_set: true,
                reason: Some("Failed to do something".to_string()),
            })
        );
    }

    #[test]
    fn a_declared_failure_without_reason_is_reported_with_a_default_reason() {
        let ttd = TempTedgeDir::new();
        let job = killed_job(&ttd);
        job.set_result(&DeclaredResult::Failed { reason: None })
            .unwrap();

        let outcome = job.collect(NO_LIMIT).unwrap().into_shell_outcome().unwrap();
        assert_eq!(outcome.exit_code, 1);
        assert_eq!(outcome.reason.as_deref(), Some(DECLARED_FAILURE_REASON));
    }

    #[test]
    fn the_output_of_an_interrupted_job_is_reported_along_the_declared_result() {
        let ttd = TempTedgeDir::new();
        let job = killed_job(&ttd);
        std::fs::write(job.output_path(), "stopping services\nrestarting").unwrap();
        job.set_result(&DeclaredResult::Successful).unwrap();

        assert_eq!(
            job.collect(NO_LIMIT).unwrap().into_shell_outcome(),
            Ok(ShellOutcome {
                result: format!("stopping services\nrestarting\n{INTERRUPTED_OUTPUT_NOTICE}"),
                exit_code: 0,
                timed_out: None,
                result_set: true,
                reason: None,
            })
        );
    }

    #[test]
    fn the_output_of_an_interrupted_job_is_truncated() {
        let ttd = TempTedgeDir::new();
        let job = killed_job(&ttd);
        std::fs::write(job.output_path(), "0123456789").unwrap();
        job.set_result(&DeclaredResult::Successful).unwrap();

        let JobOutcome::ResultSet { result, .. } = job.collect(4).unwrap() else {
            panic!("expected the declared result")
        };
        assert!(
            result.starts_with("0123\n<the output has been truncated"),
            "{result}"
        );
        assert!(result.ends_with(INTERRUPTED_OUTPUT_NOTICE), "{result}");
    }

    #[test]
    fn the_output_is_part_of_the_outcome_of_a_completed_job() {
        let ttd = TempTedgeDir::new();
        let job = created(Job::new(&data_dir(&ttd), "c8y-mapper-1234").unwrap());

        job.run(|mut output| {
            output.file.write_all(b"hello\n").unwrap();
            Ok(completed("hello\n"))
        })
        .unwrap();

        assert!(!job.output_path().exists());
    }

    #[test]
    fn job_files_left_empty_by_a_reboot_are_taken_as_missing() {
        let ttd = TempTedgeDir::new();
        let job = killed_job(&ttd);
        std::fs::write(job.outcome_path(), "").unwrap();
        std::fs::write(job.declared_result_path(), "").unwrap();

        assert_eq!(job.collect(NO_LIMIT).unwrap(), JobOutcome::Interrupted);
    }

    #[test]
    fn a_declared_result_is_used_when_the_outcome_file_is_truncated() {
        let ttd = TempTedgeDir::new();
        let job = killed_job(&ttd);
        job.set_result(&DeclaredResult::Successful).unwrap();
        std::fs::write(job.outcome_path(), r#"{"type":"compl"#).unwrap();

        assert!(matches!(
            job.collect(NO_LIMIT).unwrap(),
            JobOutcome::ResultSet {
                declared: DeclaredResult::Successful,
                ..
            }
        ));
    }

    #[test]
    fn the_actual_outcome_prevails_over_a_declared_result() {
        let ttd = TempTedgeDir::new();
        let job = created(Job::new(&data_dir(&ttd), "c8y-mapper-1234").unwrap());

        // The command declared a success, but completed, e.g. as the restart failed
        job.run(|_| {
            job.set_result(&DeclaredResult::Successful).unwrap();
            Ok(ShellOutcome {
                result: "restart failed\n".to_string(),
                exit_code: 1,
                timed_out: None,
                result_set: false,
                reason: None,
            })
        })
        .unwrap();

        assert_eq!(
            job.collect(NO_LIMIT).unwrap(),
            JobOutcome::Completed {
                result: "restart failed\n".to_string(),
                exit_code: 1,
                timed_out_ms: None
            }
        );
    }

    #[test]
    fn a_result_can_only_be_declared_by_a_running_job() {
        let ttd = TempTedgeDir::new();
        let job = Job::new(&data_dir(&ttd), "c8y-mapper-1234").unwrap();

        assert!(job.set_result(&DeclaredResult::Successful).is_err());
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
        let dir = data_dir(&ttd);

        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let job = std::thread::spawn(move || {
            created(Job::new(&dir, "c8y-mapper-1234").unwrap())
                .run(|_| {
                    started_tx.send(()).unwrap();
                    std::thread::sleep(Duration::from_millis(500));
                    Ok(completed("done\n"))
                })
                .unwrap()
        });

        // The job holds the lock while executing the command
        started_rx.recv().unwrap();

        let started = Instant::now();
        let outcome = Job::new(&data_dir(&ttd), "c8y-mapper-1234")
            .unwrap()
            .collect(NO_LIMIT)
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
        let job = created(Job::new(&data_dir(&ttd), "c8y-mapper-1234").unwrap());
        job.run(|_| Ok(completed("stale\n"))).unwrap();

        // The new run dies before storing any outcome
        let result = std::panic::catch_unwind(|| {
            job.run(|_| panic!("killed")).unwrap();
        });
        assert!(result.is_err());

        assert_eq!(job.collect(NO_LIMIT).unwrap(), JobOutcome::Interrupted);
    }

    #[test]
    fn a_command_id_cannot_escape_the_job_directory() {
        let ttd = TempTedgeDir::new();
        for cmd_id in ["", ".", "..", "../foo", "foo/bar", "foo bar", ".hidden"] {
            assert!(
                Job::new(&data_dir(&ttd), cmd_id).is_err(),
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
                Job::new(&data_dir(&ttd), cmd_id).is_ok(),
                "{cmd_id:?} should be accepted"
            );
        }
    }

    /// A job killed while running, as by a restart of the agent
    fn killed_job(ttd: &TempTedgeDir) -> Job {
        let job = Job::new(&data_dir(ttd), "c8y-mapper-1234").unwrap();
        ttd.dir("tedge-shell-plugin").dir("c8y-mapper-1234");
        job.open_lock_file().unwrap();
        job
    }

    fn data_dir(ttd: &TempTedgeDir) -> TedgePaths {
        TedgePaths::from_root_with_defaults(ttd.path(), "", "")
    }

    /// A job with its directory created, as done before running it
    fn created(job: Job) -> Job {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(job.create_dir())
            .unwrap();
        job
    }

    fn completed(result: &str) -> ShellOutcome {
        ShellOutcome {
            result: result.to_string(),
            exit_code: 0,
            timed_out: None,
            result_set: false,
            reason: None,
        }
    }
}
