use crate::config::TedgeWriteStatus;
use crate::FileConfigPlugin;
use crate::PluginConfig;
use camino::Utf8PathBuf;
use tedge_config::cli::CommonArgs;
use tedge_config::log_init;
use tedge_config::SudoCommandBuilder;

#[derive(clap::Parser, Debug)]
#[clap(
    name = clap::crate_name!(),
    version = clap::crate_version!(),
    about = clap::crate_description!(),
    arg_required_else_help(true)
)]
pub struct FileConfigCli {
    #[command(flatten)]
    pub common: CommonArgs,

    #[clap(subcommand)]
    operation: PluginOp,
}

#[derive(clap::Subcommand, Debug)]
pub enum PluginOp {
    /// List all available configuration types
    List,

    /// Get configuration for a specific type
    Get {
        /// Configuration type to retrieve
        config_type: String,
    },

    /// Set configuration for a specific type
    Set {
        /// Configuration type to update
        config_type: String,

        /// Path to the new configuration file
        config_path: String,
    },
}

#[derive(Debug)]
pub struct TEdgeConfigView {
    pub is_sudo_enabled: bool,
}

impl TEdgeConfigView {
    pub fn new(is_sudo_enabled: bool) -> Self {
        Self { is_sudo_enabled }
    }
}

pub fn run(cli: FileConfigCli, tedge_config: TEdgeConfigView) -> anyhow::Result<()> {
    if let Err(err) = log_init(
        "tedge-file-config-plugin",
        &cli.common.log_args,
        &cli.common.config_dir,
    ) {
        log::error!("Can't enable logging due to error: {err}");
        return Err(err.into());
    }

    let config_dir = cli.common.config_dir;
    let config_path = config_dir
        .join("plugins")
        .join("tedge-configuration-plugin.toml");

    let plugin_config = PluginConfig::new(&config_path);

    let use_tedge_write = TedgeWriteStatus::Enabled {
        sudo: SudoCommandBuilder::enabled(tedge_config.is_sudo_enabled),
    };

    let plugin = FileConfigPlugin::new(plugin_config, use_tedge_write);

    match cli.operation {
        PluginOp::List => {
            let types = plugin.list()?;
            for config_type in types {
                println!("{}", config_type);
            }
            Ok(())
        }
        PluginOp::Get { config_type } => plugin.get(&config_type).map_err(|err| {
            log::error!("Failed to get configuration for {config_type} : {err}");
            err.into()
        }),
        PluginOp::Set {
            config_type,
            config_path,
        } => {
            let source_path = Utf8PathBuf::from(config_path);
            match plugin.set(&config_type, &source_path) {
                Ok(()) => {
                    log::info!("Successfully updated configuration for {}", config_type);
                    Ok(())
                }
                Err(err) => {
                    log::error!("Failed to set configuration: {err}");
                    Err(err.into())
                }
            }
        }
    }
}
