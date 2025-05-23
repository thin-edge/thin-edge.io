use crate::cli::diag::collect::DiagCollectCommand;
use crate::command::BuildCommand;
use crate::command::Command;
use crate::ConfigError;
use camino::Utf8PathBuf;
use tedge_config::models::AbsolutePath;
use tedge_config::models::SecondsOrHumanTime;
use tedge_config::TEdgeConfig;
use time::format_description;
use time::OffsetDateTime;

#[derive(clap::Subcommand, Debug)]
pub enum TEdgeDiagCli {
    /// Collect diagnostic information by running device-specific scripts
    Collect {
        /// Directory where diagnostic plugins are stored
        ///
        /// [default: <CONFIG_DIR>/diag-plugins]
        #[clap(long)]
        plugin_dir: Option<Utf8PathBuf>,

        /// Directory where output tarball and temporary output files are stored. The path from tmp.path will be used by default
        #[clap(long)]
        output_dir: Option<Utf8PathBuf>,

        /// Filename (without .tar.gz) for the output tarball
        ///
        /// [default: tedge-diag_<TIMESTAMP>]
        #[clap(long)]
        tarball_name: Option<String>,

        /// Whether to keep intermediate output files after the tarball is created
        #[clap(long)]
        keep_dir: bool,

        /// Timeout for a graceful plugin shutdown
        #[clap(long, default_value = "60s")]
        timeout: SecondsOrHumanTime,

        /// Timeout for forced termination, starting after a graceful timeout expires
        #[clap(long, default_value = "60s")]
        forceful_timeout: SecondsOrHumanTime,
    },
}

impl BuildCommand for TEdgeDiagCli {
    fn build_command(self, config: &TEdgeConfig) -> Result<Box<dyn Command>, ConfigError> {
        match self {
            TEdgeDiagCli::Collect {
                plugin_dir,
                output_dir,
                tarball_name,
                keep_dir,
                timeout,
                forceful_timeout,
            } => {
                let plugin_dir =
                    plugin_dir.unwrap_or_else(|| config.root_dir().join("diag-plugins"));
                let output_dir = output_dir.unwrap_or_else(|| config.tmp.path.to_path_buf());
                let now = OffsetDateTime::now_utc()
                    .format(
                        &format_description::parse("[year]-[month]-[day]_[hour]-[minute]-[second]")
                            .unwrap(),
                    )
                    .unwrap();
                let tarball_name = tarball_name.unwrap_or(format!("tedge-diag-{now}"));

                let cmd = DiagCollectCommand {
                    plugin_dir: get_absolute_path(plugin_dir)?,
                    config_dir: get_absolute_path(config.root_dir().to_path_buf())?,
                    working_dir: get_absolute_path(output_dir.clone())?,
                    diag_dir: get_absolute_path(output_dir.join(&tarball_name))?,
                    tarball_name,
                    keep_dir,
                    graceful_timeout: timeout.duration(),
                    forceful_timeout: forceful_timeout.duration(),
                }
                .into_boxed();
                Ok(cmd)
            }
        }
    }
}

fn get_absolute_path(path: Utf8PathBuf) -> Result<AbsolutePath, anyhow::Error> {
    Ok(AbsolutePath::from_path(path)?)
}
