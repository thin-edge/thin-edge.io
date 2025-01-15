//! Common CLI arguments and helpers used by all thin-edge components.

use camino::Utf8PathBuf;
use clap::Args;
use clap::ValueHint;

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
    pub log_level: Option<tracing::Level>,
}
