use crate::execute_to_file;
use crate::job::DeclaredResult;
use crate::job::Job;
use crate::job::JobOutcome;
use crate::write_launch_error;
use crate::write_script_output;
use crate::OutputFile;
use crate::ShellOutcome;
use camino::Utf8PathBuf;
use std::io::Write;
use std::time::Duration;
use tedge_config::cli::CommonArgs;
use tedge_config::log_init;
use tedge_config::TEdgeConfig;
use tedge_utils::paths::TedgePaths;
use tracing::error;
use tracing::info;

#[derive(clap::Parser, Debug)]
#[clap(
    name = clap::crate_name!(),
    version = clap::crate_version!(),
    about = clap::crate_description!(),
    arg_required_else_help(true)
)]
pub struct ShellCli {
    #[command(flatten)]
    pub common: CommonArgs,

    #[command(subcommand)]
    action: ShellAction,
}

#[derive(clap::Subcommand, Debug)]
enum ShellAction {
    /// Execute a command in the background, storing its outcome for a later `collect`
    ///
    /// Nothing is reported by this command, whose output is ignored by the workflow engine.
    Execute {
        /// The identifier of the command, used to find its outcome
        #[clap(long = "cmd-id")]
        cmd_id: String,

        #[command(flatten)]
        run: RunArgs,
    },

    /// Wait for a command started with `execute` to complete, and report its outcome
    Collect {
        /// The identifier of the command
        #[clap(long = "cmd-id")]
        cmd_id: String,
    },

    /// Set, from within a command started with `execute`, the result reported should it be interrupted
    ///
    /// A command restarting the tedge-agent or the device is usually killed along with the agent,
    /// and then reported as failed. Called before the restart, this reports the command
    /// with the given result instead, e.g.
    /// `tedge-shell-plugin set-result --outcome successful && sudo tedge service restart tedge-agent`.
    ///
    /// A command which completes anyway, e.g. because the restart failed,
    /// is reported with its actual outcome.
    SetResult {
        /// The identifier of the command
        #[clap(long = "cmd-id", env = CMD_ID_ENV)]
        cmd_id: String,

        /// The outcome reported for the command
        #[clap(long, value_enum)]
        outcome: Outcome,

        /// The reason reported for a failed outcome, used as is as the failure reason
        #[clap(long)]
        reason: Option<String>,
    },
}

#[derive(clap::ValueEnum, Clone, Copy, Debug)]
enum Outcome {
    Successful,
    Failed,
}

/// The environment variable giving a command started with `execute` its identifier
///
/// Not prefixed with `TEDGE_`, as such variables are taken as tedge config settings,
/// an unknown one being warned about by any tedge command run by the command.
const CMD_ID_ENV: &str = "SHELL_EXECUTE_CMD_ID";

/// The environment variable giving a command started with `execute` the tedge config directory,
/// so `set-result` finds the job files in the same data dir
const CONFIG_DIR_ENV: &str = "TEDGE_CONFIG_DIR";

#[derive(clap::Args, Debug)]
struct RunArgs {
    /// The command to be executed by the shell
    #[clap(long = "command", allow_hyphen_values = true)]
    command: Option<String>,

    /// The shell used to execute the command
    ///
    /// Defaults to the `shell.path` tedge configuration setting.
    #[clap(long = "shell")]
    shell: Option<Utf8PathBuf>,
}

/// The `tedge config` settings used by this plugin
#[derive(Debug)]
pub struct TEdgeConfigView {
    pub shell: Utf8PathBuf,
    /// The directory of the files of the commands run in the background,
    /// which is expected to survive a device reboot
    pub data_dir: TedgePaths,
    pub max_output_size: u32,
    pub timeout: Duration,
}

impl TEdgeConfigView {
    pub fn new(tedge_config: &TEdgeConfig) -> Self {
        let system = tedge_config.read_system_config();
        TEdgeConfigView {
            shell: tedge_config.shell.path.to_path_buf(),
            // So the job directories are owned as the other thin-edge directories
            data_dir: TedgePaths::from_root_with_defaults(
                tedge_config.data.path.clone(),
                system.user,
                system.group,
            ),
            max_output_size: tedge_config.shell.max_output_size,
            timeout: tedge_config.shell.timeout.duration(),
        }
    }
}

pub async fn run(cli: ShellCli, config: TEdgeConfigView) -> anyhow::Result<()> {
    if let Err(err) = log_init(
        "tedge-shell-plugin",
        &cli.common.log_args,
        &cli.common.config_dir,
    ) {
        error!("Can't enable logging due to error: {err}");
        return Err(err.into());
    }

    match cli.action {
        ShellAction::Execute { cmd_id, run } => {
            // The job files cannot be stored, so the reason is reported by the collector,
            // with no attempt to create the data dir: this is up to the device administrator
            check_data_dir(&config.data_dir).map_err(anyhow::Error::msg)?;
            let job = Job::new(&config.data_dir, &cmd_id)?;
            job.create_dir().await?;
            let config_dir = cli.common.config_dir;
            tokio::task::spawn_blocking(move || {
                let envs = [
                    (CMD_ID_ENV, cmd_id.as_str()),
                    (CONFIG_DIR_ENV, config_dir.as_str()),
                ];
                job.run(|output| run_command(run, &config, output, &envs))
            })
            .await??;
            Ok(())
        }
        ShellAction::Collect { cmd_id } => {
            // Without a data dir, no job has been run and the command would be reported as interrupted
            if let Err(reason) = check_data_dir(&config.data_dir) {
                return report(Err(reason));
            }
            let job = Job::new(&config.data_dir, &cmd_id)?;
            let max_output_size = config.max_output_size;
            let outcome = tokio::task::spawn_blocking(move || job.collect(max_output_size))
                .await?
                .map_err(|err| {
                    format!(
                        "Failed to collect the command outcome from the data dir '{}': {err}",
                        config.data_dir.root()
                    )
                });
            report(outcome.and_then(JobOutcome::into_shell_outcome))
        }
        ShellAction::SetResult {
            cmd_id,
            outcome,
            reason,
        } => {
            let declared = match (outcome, reason) {
                (Outcome::Successful, None) => DeclaredResult::Successful,
                (Outcome::Successful, Some(_)) => {
                    anyhow::bail!("A reason can only be given for a failed outcome")
                }
                (Outcome::Failed, reason) => DeclaredResult::Failed { reason },
            };
            let job = Job::new(&config.data_dir, &cmd_id)?;
            tokio::task::spawn_blocking(move || job.set_result(&declared)).await??;
            Ok(())
        }
    }
}

/// Check that the directory used to store the command output exists
fn check_data_dir(dir: &TedgePaths) -> Result<(), String> {
    let dir = dir.root();
    if dir.is_dir() {
        Ok(())
    } else {
        Err(format!("the configured data.path '{dir}' does not exist"))
    }
}

/// Run the command, returning either its outcome or the reason it could not be run
fn run_command(
    args: RunArgs,
    config: &TEdgeConfigView,
    output: OutputFile,
    envs: &[(&str, &str)],
) -> Result<ShellOutcome, String> {
    let command = args.command.unwrap_or_default();
    if command.trim().is_empty() {
        return Err("No command to execute".to_string());
    }

    let shell = args.shell.as_ref().unwrap_or(&config.shell);
    info!("Executing command using shell: {shell}");

    execute_to_file(
        shell,
        &command,
        output,
        config.max_output_size,
        config.timeout,
        envs,
    )
    .map_err(|err| format!("Failed to run the command using the shell '{shell}': {err}"))
}

/// Report the outcome of a command using the script output protocol, and exit accordingly
fn report(outcome: Result<ShellOutcome, String>) -> anyhow::Result<()> {
    let mut stdout = std::io::stdout();
    let outcome = match outcome {
        Ok(outcome) => outcome,
        Err(reason) => {
            // Reporting the reason using the script output protocol, as the workflow engine
            // otherwise only tells the user that this plugin returned a non-zero exit code
            write_launch_error(&mut stdout, &reason)?;
            stdout.flush()?;
            anyhow::bail!(reason);
        }
    };

    write_script_output(&mut stdout, &outcome)?;
    stdout.flush()?;

    if outcome.result_set {
        info!("Command interrupted, reporting the result it set before the interruption");
    } else if let Some(timeout) = outcome.timed_out {
        info!(
            "Command terminated as it did not complete within {}",
            humantime::format_duration(timeout)
        );
    } else if outcome.exit_code != 0 {
        info!(
            "Command returned a non-zero exit code. code={}",
            outcome.exit_code
        );
    }

    // Propagate the command exit code, so the workflow can tell success from failure
    std::process::exit(outcome.exit_code)
}
