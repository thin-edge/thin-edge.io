//! Common CLI arguments and helpers used by all thin-edge components.

use camino::Utf8PathBuf;
use clap::Args;
use clap::ValueHint;
use clap_complete::ArgValueCandidates;
use clap_complete::CompletionCandidate;

/// Formats a `tedge config set` command for a built-in mapper key.
///
/// The `mapper_name` may be bare (e.g. `"c8y"`) or profile-qualified
/// (e.g. `"c8y.prod"`). In the latter case the profile is passed via
/// `--profile` so the command is valid for the named profile.
pub fn format_config_set_cmd(mapper_name: &str, config_key: &str) -> String {
    match mapper_name.split_once('.') {
        Some((cloud, profile)) => {
            format!("tedge config set {cloud}.{config_key} <value> --profile {profile}")
        }
        None => format!("tedge config set {mapper_name}.{config_key} <value>"),
    }
}

/// CLI arguments that should be handled by all thin-edge components.
#[derive(Args, Debug, PartialEq, Eq, Clone)]
pub struct CommonArgs {
    /// [env: TEDGE_CONFIG_DIR, default: /etc/tedge]
    #[clap(
            long = "config-dir",
            default_value = crate::get_config_dir().into_os_string(),
            hide_env_values = true,
            hide_default_value = true,
            global = true,
            value_hint = ValueHint::DirPath,
        )]
    pub config_dir: Utf8PathBuf,

    #[command(flatten)]
    pub log_args: LogConfigArgs,
}

#[derive(Args, Debug, PartialEq, Eq, Clone)]
pub struct LogConfigArgs {
    /// Turn-on the DEBUG log level.
    ///
    /// If off only reports ERROR, WARN, and INFO, if on also reports DEBUG
    #[clap(long, global = true)]
    pub debug: bool,

    /// Configures the logging level.
    ///
    /// One of error/warn/info/debug/trace. Logs with verbosity lower or equal to the selected level
    /// will be printed, i.e. warn prints ERROR and WARN logs and trace prints logs of all levels.
    ///
    /// Overrides `--debug`
    #[clap(long, global = true)]
    #[clap(add(ArgValueCandidates::new(log_level_completions)))]
    pub log_level: Option<tracing::Level>,
}

fn log_level_completions() -> Vec<CompletionCandidate> {
    use tracing::Level as L;
    let options = [L::TRACE, L::DEBUG, L::INFO, L::WARN, L::ERROR];
    options
        .into_iter()
        .map(|level| CompletionCandidate::new(level.to_string().to_lowercase()))
        .collect()
}
