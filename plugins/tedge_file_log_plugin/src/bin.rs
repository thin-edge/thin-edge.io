use crate::FileLogPlugin;
use crate::LogPluginConfig;
use camino::Utf8Path;
use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;
use tedge_config::cli::CommonArgs;
use tedge_config::log_init;
use time::OffsetDateTime;

#[derive(clap::Parser, Debug)]
#[clap(
    name = clap::crate_name!(),
    version = clap::crate_version!(),
    about = clap::crate_description!(),
    arg_required_else_help(true)
)]
pub struct FileLogCli {
    #[command(flatten)]
    pub common: CommonArgs,

    #[clap(subcommand)]
    operation: PluginOp,
}

#[derive(clap::Subcommand, Debug)]
pub enum PluginOp {
    /// List all available log types
    List,

    /// Get logs for a specific type
    Get {
        /// Log type to retrieve
        log_type: String,

        /// Filter logs from this date onwards
        #[clap(long = "since")]
        since: Option<String>,

        /// Filter logs up to this date
        #[clap(long = "until")]
        until: Option<String>,
    },
}

#[derive(Debug)]
pub struct TEdgeConfigView {
    pub tmp_dir: Arc<Utf8Path>,
}

impl TEdgeConfigView {
    pub fn new(tmp_dir: &Utf8Path) -> Self {
        Self {
            tmp_dir: Arc::from(tmp_dir),
        }
    }
}

pub fn run(cli: FileLogCli, plugin_config: TEdgeConfigView) -> anyhow::Result<()> {
    if let Err(err) = log_init(
        "tedge-file-log-plugin",
        &cli.common.log_args,
        &cli.common.config_dir,
    ) {
        log::error!("Can't enable logging due to error: {err}");
        return Err(err.into());
    }

    let config_dir = Path::new(&cli.common.config_dir);
    let config_path = config_dir.join("plugins").join("tedge-log-plugin.toml");

    let config = LogPluginConfig::new(&config_path);
    let plugin = FileLogPlugin::new(config, plugin_config.tmp_dir);

    match cli.operation {
        PluginOp::List => match plugin.list(None) {
            Ok(types) => {
                for log_type in types {
                    println!("{}", log_type);
                }
                Ok(())
            }
            Err(err) => {
                log::error!("Failed to list log types: {err}");
                Err(err.into())
            }
        },
        PluginOp::Get {
            log_type,
            since,
            until,
        } => {
            let since_date = if let Some(since_str) = since {
                match parse_date(&since_str) {
                    Ok(date) => Some(date),
                    Err(err) => {
                        log::error!("Invalid since date: {err}");
                        return Err(err);
                    }
                }
            } else {
                None
            };

            let until_date = if let Some(until_str) = until {
                match parse_date(&until_str) {
                    Ok(date) => Some(date),
                    Err(err) => {
                        log::error!("Invalid until date: {err}");
                        return Err(err);
                    }
                }
            } else {
                None
            };

            match plugin.get(&log_type, since_date, until_date) {
                Ok(log_path) => {
                    let src = File::open(&log_path)?;
                    let reader = BufReader::new(src);

                    for line in reader.lines() {
                        let line = line?;
                        println!("{}", line);
                    }
                    Ok(())
                }
                Err(err) => {
                    log::error!("Failed to get logs: {err}");
                    Err(err.into())
                }
            }
        }
    }
}

fn parse_date(date_str: &str) -> anyhow::Result<OffsetDateTime> {
    let timestamp = date_str.parse::<i64>()?;
    Ok(OffsetDateTime::from_unix_timestamp(timestamp)?)
}
