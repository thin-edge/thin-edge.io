use crate::execute;
use crate::job::Job;
use crate::job::JobOutcome;
use crate::write_launch_error;
use crate::write_script_output;
use crate::ShellOutcome;
use camino::Utf8PathBuf;
use std::io::Write;
use std::time::Duration;
use tedge_config::cli::CommonArgs;
use tedge_config::log_init;
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
    action: Option<ShellAction>,

    #[command(flatten)]
    run: RunArgs,
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
}

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
    pub tmp_dir: Utf8PathBuf,
    pub max_output_size: u32,
    pub timeout: Duration,
}

pub fn run(cli: ShellCli, config: TEdgeConfigView) -> anyhow::Result<()> {
    if let Err(err) = log_init(
        "tedge-shell-plugin",
        &cli.common.log_args,
        &cli.common.config_dir,
    ) {
        error!("Can't enable logging due to error: {err}");
        return Err(err.into());
    }

    if cli.action.is_some() && (cli.run.command.is_some() || cli.run.shell.is_some()) {
        anyhow::bail!("The --command and --shell options must be given after the subcommand");
    }

    match cli.action {
        None => {
            let outcome = check_tmp_dir(&config).and_then(|()| run_command(cli.run, &config));
            report(outcome)
        }
        Some(ShellAction::Execute { cmd_id, run }) => {
            // The job files cannot be stored, so the reason is reported by the collector,
            // with no attempt to create the tmp dir: this is up to the device administrator
            check_tmp_dir(&config).map_err(anyhow::Error::msg)?;
            let job = Job::new(&config.tmp_dir, &cmd_id)?;
            job.run(|| run_command(run, &config))?;
            Ok(())
        }
        Some(ShellAction::Collect { cmd_id }) => {
            // Without a tmp dir, no job has been run and the command would be reported as interrupted
            if let Err(reason) = check_tmp_dir(&config) {
                return report(Err(reason));
            }
            let job = Job::new(&config.tmp_dir, &cmd_id)?;
            let outcome = job.collect().map_err(|err| {
                format!(
                    "Failed to collect the command outcome from the tmp dir '{}': {err}",
                    config.tmp_dir
                )
            });
            report(outcome.and_then(JobOutcome::into_shell_outcome))
        }
    }
}

/// Check that the tmp dir, used to store the command output, exists
fn check_tmp_dir(config: &TEdgeConfigView) -> Result<(), String> {
    if config.tmp_dir.is_dir() {
        Ok(())
    } else {
        Err(format!(
            "the configured tmp.path '{}' does not exist",
            config.tmp_dir
        ))
    }
}

/// Run the command, returning either its outcome or the reason it could not be run
fn run_command(args: RunArgs, config: &TEdgeConfigView) -> Result<ShellOutcome, String> {
    let command = args.command.unwrap_or_default();
    if command.trim().is_empty() {
        return Err("No command to execute".to_string());
    }

    let shell = args.shell.as_ref().unwrap_or(&config.shell);
    info!("Executing command using shell: {shell}");

    execute(
        shell,
        &command,
        &config.tmp_dir,
        config.max_output_size,
        config.timeout,
    )
    .map_err(|err| {
        format!(
            "Failed to run the command using the shell '{shell}' and the tmp dir '{}': {err}",
            config.tmp_dir
        )
    })
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

    if let Some(timeout) = outcome.timed_out {
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
