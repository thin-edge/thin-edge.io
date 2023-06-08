use std::path::PathBuf;

pub use self::certificate::*;
use crate::command::BuildCommand;
use crate::command::BuildContext;
use crate::command::Command;
pub use connect::*;
use tedge_config::DEFAULT_TEDGE_CONFIG_PATH;

use self::init::TEdgeInitCmd;

mod certificate;
mod common;
pub mod config;
mod connect;
mod disconnect;
mod init;
mod mqtt;
mod reconnect;

#[derive(clap::Parser, Debug)]
#[clap(
    name = clap::crate_name!(),
    version = clap::crate_version!(),
    about = clap::crate_description!(),
    arg_required_else_help(true)
)]

pub struct Opt {
    /// Initialize the tedge
    #[clap(long)]
    pub init: bool,

    #[clap(long = "config-dir", default_value = DEFAULT_TEDGE_CONFIG_PATH)]
    pub config_dir: PathBuf,

    #[clap(subcommand)]
    pub tedge: Option<TEdgeOpt>,
}

#[derive(clap::Subcommand, Debug)]
pub enum TEdgeOpt {
    /// Initialize Thin Edge
    Init {
        /// The user who will own the directories created
        #[clap(long, default_value = "tedge")]
        user: String,

        /// The group who will own the directories created
        #[clap(long, default_value = "tedge")]
        group: String,
    },

    /// Create and manage device certificate
    #[clap(subcommand)]
    Cert(certificate::TEdgeCertCli),

    /// Configure Thin Edge.
    #[clap(subcommand)]
    Config(config::ConfigCmd),

    /// Connect to connector provider
    #[clap(subcommand)]
    Connect(connect::TEdgeConnectOpt),

    /// Remove bridge connection for a provider
    #[clap(subcommand)]
    Disconnect(disconnect::TEdgeDisconnectBridgeCli),

    /// Reconnect command, calls disconnect followed by connect
    #[clap(subcommand)]
    Reconnect(reconnect::TEdgeReconnectCli),

    /// Publish a message on a topic and subscribe a topic.
    #[clap(subcommand)]
    Mqtt(mqtt::TEdgeMqttCli),
}

impl BuildCommand for TEdgeOpt {
    fn build_command(self, context: BuildContext) -> Result<Box<dyn Command>, crate::ConfigError> {
        match self {
            TEdgeOpt::Init { user, group } => Ok(Box::new(TEdgeInitCmd::new(user, group, context))),
            TEdgeOpt::Cert(opt) => opt.build_command(context),
            TEdgeOpt::Config(opt) => opt.build_command(context),
            TEdgeOpt::Connect(opt) => opt.build_command(context),
            TEdgeOpt::Disconnect(opt) => opt.build_command(context),
            TEdgeOpt::Mqtt(opt) => opt.build_command(context),
            TEdgeOpt::Reconnect(opt) => opt.build_command(context),
        }
    }
}
