use std::path::PathBuf;

use crate::command::{BuildCommand, BuildContext, Command};
use tedge_config::DEFAULT_TEDGE_CONFIG_PATH;

mod certificate;
mod config;
mod connect;
mod disconnect;
mod mqtt;

#[derive(clap::Parser, Debug)]
#[clap(
    name = clap::crate_name!(),
    version = clap::crate_version!(),
    about = clap::crate_description!(),
    arg_required_else_help(true)
)]

pub struct Opt {
    #[clap(short, long)]
    pub init: bool,

    #[clap(long = "config-dir", default_value = DEFAULT_TEDGE_CONFIG_PATH)]
    pub config_dir: PathBuf,

    #[clap(subcommand)]
    pub tedge: Option<TEdgeOpt>,
}

#[derive(clap::Subcommand, Debug)]
pub enum TEdgeOpt {
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

    /// Publish a message on a topic and subscribe a topic.
    #[clap(subcommand)]
    Mqtt(mqtt::TEdgeMqttCli),
}

impl BuildCommand for TEdgeOpt {
    fn build_command(self, context: BuildContext) -> Result<Box<dyn Command>, crate::ConfigError> {
        match self {
            TEdgeOpt::Cert(opt) => opt.build_command(context),
            TEdgeOpt::Config(opt) => opt.build_command(context),
            TEdgeOpt::Connect(opt) => opt.build_command(context),
            TEdgeOpt::Disconnect(opt) => opt.build_command(context),
            TEdgeOpt::Mqtt(opt) => opt.build_command(context),
        }
    }
}
