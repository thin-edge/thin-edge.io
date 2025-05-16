use crate::cli::diag::collect::DiagCollectCommand;
use crate::command::BuildCommand;
use crate::command::Command;
use crate::ConfigError;
use camino::Utf8PathBuf;
use tedge_config::models::AbsolutePath;
use tedge_config::models::SecondsOrHumanTime;
use tedge_config::TEdgeConfig;
use tedge_config::TEdgeConfigLocation;
use time::format_description;
use time::OffsetDateTime;

#[derive(clap::Subcommand, Debug)]
pub enum TEdgeDiagCli {
    /// Collect diagnostic logs
    Collect {
        /// Directory where diagnostic plugins are stored
        #[clap(long, default_value = "/etc/tedge/diag-plugins")]
        plugin_dir: Utf8PathBuf,

        /// Directory where output tarball and temporary output files are stored. The path from tmp.path will be used by default
        #[clap(long)]
        output_dir: Option<Utf8PathBuf>,

        /// Filename (without .tar.gz) for the output tarball
        /// [default: tedge-diag_<timestamp>]
        #[clap(long)]
        tarball_name: Option<String>,

        /// Timeout for a graceful plugin shutdown
        #[clap(long, default_value = "60s")]
        graceful_timeout: SecondsOrHumanTime,

        /// Timeout for forced termination, starting after a graceful timeout expires
        #[clap(long, default_value = "60s")]
        forceful_timeout: SecondsOrHumanTime,
    },
}

impl BuildCommand for TEdgeDiagCli {
    fn build_command(
        self,
        config: TEdgeConfig,
        config_location: TEdgeConfigLocation,
    ) -> Result<Box<dyn Command>, ConfigError> {
        match self {
            TEdgeDiagCli::Collect {
                plugin_dir,
                output_dir,
                tarball_name,
                graceful_timeout,
                forceful_timeout,
            } => {
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
                    diag_dir: get_absolute_path(output_dir.join(&tarball_name))?,
                    config_dir: get_absolute_path(config_location.tedge_config_root_path)?,
                    graceful_timeout: graceful_timeout.duration(),
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
