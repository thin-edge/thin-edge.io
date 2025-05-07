use crate::cli::diag::collect::DiagCollectCommand;
use crate::command::BuildCommand;
use crate::command::Command;
use crate::ConfigError;
use camino::Utf8PathBuf;
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

        /// Directory where output tarball and temporary output files are stored
        /// [env: TEDGE_TMP_PATH]
        #[clap(long, default_value = "/tmp")]
        output_dir: Utf8PathBuf,

        /// Filename (without .tar.gz) for the output tarball
        /// [default: tedge-diag_<timestamp>]
        #[clap(long)]
        tarball_name: Option<String>,

        /// Timeout for each plugin's execution
        #[clap(long, default_value = "10s")]
        timeout: SecondsOrHumanTime,
    },
}

impl BuildCommand for TEdgeDiagCli {
    fn build_command(
        self,
        _config: TEdgeConfig,
        config_location: TEdgeConfigLocation,
    ) -> Result<Box<dyn Command>, ConfigError> {
        match self {
            TEdgeDiagCli::Collect {
                plugin_dir,
                output_dir,
                tarball_name,
                timeout,
            } => {
                let now = OffsetDateTime::now_utc()
                    .format(&format_description::well_known::Rfc3339)
                    .unwrap();
                let tarball_name = tarball_name.unwrap_or(format!("tedge-diag-{now}"));

                let cmd = DiagCollectCommand {
                    plugin_dir,
                    diag_dir: output_dir.join(&tarball_name),
                    config_dir: config_location.tedge_config_root_path,
                    timeout,
                }
                .into_boxed();
                Ok(cmd)
            }
        }
    }
}
