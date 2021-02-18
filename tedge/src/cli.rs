use super::command::Command;
use crate::config;
use crate::config::{ConfigError, TEdgeConfig};
use structopt::clap;
use structopt::StructOpt;

mod connect;
mod disconnect;

pub trait BuildCommand {
    fn build_command(
        self,
        config: &config::TEdgeConfig,
    ) -> Result<Box<dyn Command>, config::ConfigError>;
}

#[derive(StructOpt, Debug)]
#[structopt(
    name = clap::crate_name!(),
    version = clap::crate_version!(),
    about = clap::crate_description!()
)]
pub struct Opt {
    // The number of occurrences of the `v` flag
    /// Verbose mode (-v, -vv, -vvv, etc.)
    #[structopt(short, parse(from_occurrences))]
    pub verbose: u8,

    #[structopt(subcommand)]
    pub tedge_opt: TEdgeOpt,
}

#[derive(StructOpt, Debug)]
pub enum TEdgeOpt {
    /// Create and manage device certificate
    Cert(super::certificate::CertOpt),

    /// Configure Thin Edge.
    Config(config::ConfigCmd),

    /// Connect to connector provider
    Connect(connect::ConnectCmd),

    /// Remove bridge connection for a provider
    Disconnect(disconnect::DisconnectCmd),

    /// Publish a message on a topic and subscribe a topic.
    Mqtt(super::mqtt::MqttCmd),
}

impl BuildCommand for TEdgeOpt {
    fn build_command(self, config: &TEdgeConfig) -> Result<Box<dyn Command>, ConfigError> {
        match self {
            TEdgeOpt::Cert(opt) => opt.build_command(config),
            TEdgeOpt::Config(opt) => opt.build_command(config),
            TEdgeOpt::Connect(opt) => opt.build_command(config),
            TEdgeOpt::Disconnect(opt) => opt.build_command(config),
            TEdgeOpt::Mqtt(opt) => opt.build_command(config),
        }
    }
}
