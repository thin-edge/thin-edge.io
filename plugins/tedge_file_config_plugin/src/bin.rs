use crate::config::TedgeWriteStatus;
use crate::FileConfigPlugin;
use crate::PluginConfig;
use camino::Utf8PathBuf;
use tedge_config::cli::CommonArgs;
use tedge_config::log_init;
use tedge_config::SudoCommandBuilder;
use tedge_system_services::GeneralServiceManager;

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

    /// Prepare for configuration update (e.g: create backup)
    Prepare {
        /// Configuration type to prepare
        config_type: String,

        from_path: String,

        /// Working directory for metadata
        #[clap(long = "work-dir")]
        work_dir: String,
    },

    /// Set configuration for a specific type
    Set {
        /// Configuration type to update
        config_type: String,

        /// Path to the new configuration file
        config_path: String,

        /// Working directory for metadata
        #[clap(long = "work-dir")]
        work_dir: String,
    },

    /// Verify configuration was applied successfully
    Verify {
        /// Configuration type to verify
        config_type: String,

        /// Working directory with metadata
        #[clap(long = "work-dir")]
        work_dir: String,
    },

    /// Rollback configuration to previous state
    Rollback {
        /// Configuration type to rollback
        config_type: String,

        /// Working directory with backup
        #[clap(long = "work-dir")]
        work_dir: String,
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

pub async fn run(cli: FileConfigCli, tedge_config: TEdgeConfigView) -> anyhow::Result<()> {
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

    let service_manager = GeneralServiceManager::try_new(&config_dir)?;

    let plugin = FileConfigPlugin::new(plugin_config, use_tedge_write, service_manager);

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
        PluginOp::Prepare {
            config_type,
            from_path,
            work_dir,
        } => {
            let workdir_path = Utf8PathBuf::from(work_dir);
            let new_config_path = Utf8PathBuf::from(from_path);
            match plugin
                .prepare(&config_type, &workdir_path, &new_config_path)
                .await
            {
                Ok(()) => {
                    log::info!("Successfully prepared configuration for {}", config_type);
                    Ok(())
                }
                Err(err) => {
                    log::error!("Failed to prepare configuration: {err}");
                    Err(err.into())
                }
            }
        }
        PluginOp::Set {
            config_type,
            config_path,
            work_dir: _,
        } => {
            let source_path = Utf8PathBuf::from(config_path);
            match plugin.set(&config_type, &source_path).await {
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
        PluginOp::Verify {
            config_type,
            work_dir,
        } => {
            let workdir_path = Utf8PathBuf::from(work_dir);
            match plugin.verify(&config_type, &workdir_path).await {
                Ok(()) => {
                    log::info!("Successfully verified configuration for {}", config_type);
                    Ok(())
                }
                Err(err) => {
                    log::error!("Failed to verify configuration: {err}");
                    Err(err.into())
                }
            }
        }
        PluginOp::Rollback {
            config_type,
            work_dir,
        } => {
            let workdir_path = Utf8PathBuf::from(work_dir);
            match plugin.rollback(&config_type, &workdir_path).await {
                Ok(()) => {
                    log::info!("Successfully rolled back configuration for {}", config_type);
                    Ok(())
                }
                Err(err) => {
                    log::error!("Failed to rollback configuration: {err}");
                    Err(err.into())
                }
            }
        }
    }
}
