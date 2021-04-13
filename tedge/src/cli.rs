use crate::certificate;
use crate::command::{BuildCommand, Command};
use crate::config;
use crate::config::{ConfigError, TEdgeConfig};
use structopt::clap;
use structopt::StructOpt;

mod connect;
mod disconnect;
mod mqtt;

#[derive(StructOpt, Debug)]
#[structopt(
    name = clap::crate_name!(),
    version = clap::crate_version!(),
    about = clap::crate_description!()
)]
pub struct Opt {
    #[structopt(subcommand)]
    pub tedge: TEdgeOpt,
}

#[derive(StructOpt, Debug)]
pub enum TEdgeOpt {
    /// Create and manage device certificate
    Cert(certificate::TEdgeCertOpt),

    /// Configure Thin Edge.
    Config(config::ConfigCmd),

    /// Connect to connector provider
    Connect(connect::TEdgeConnectOpt),

    /// Remove bridge connection for a provider
    Disconnect(disconnect::TedgeDisconnectBridgeOpt),

    /// Publish a message on a topic and subscribe a topic.
    Mqtt(mqtt::TEdgeMqttCli),
}

impl BuildCommand for TEdgeOpt {
    fn build_command(self, config: TEdgeConfig) -> Result<Box<dyn Command>, ConfigError> {
        match self {
            TEdgeOpt::Cert(opt) => opt.build_command(config),
            TEdgeOpt::Config(opt) => opt.build_command(config),
            TEdgeOpt::Connect(opt) => opt.build_command(config),
            TEdgeOpt::Disconnect(opt) => opt.build_command(config),
            TEdgeOpt::Mqtt(opt) => opt.build_command(config),
        }
    }
}
