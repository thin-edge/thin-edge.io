use super::command::Command;
use crate::config;
use structopt::clap;
use structopt::StructOpt;
use crate::config::{ConfigError, TEdgeConfig};

mod connect;
mod disconnect;

pub trait CliOption {
    fn build_command(&self, config: &config::TEdgeConfig) -> Result<Box<dyn Command>, config::ConfigError>;
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
    Cert(super::certificate::CertCmd),

    /// Configure Thin Edge.
    Config(config::ConfigCmd),

    /// Connect to connector provider
    Connect(connect::ConnectCmd),

    /// Remove bridge connection for a provider
    Disconnect(disconnect::DisconnectCmd),

    /// Publish a message on a topic and subscribe a topic.
    Mqtt(super::mqtt::MqttCmd),
}

impl TEdgeOpt {
    fn sub_option(&self) -> &dyn CliOption {
        match self {
            TEdgeOpt::Cert(ref cmd) => cmd,
            TEdgeOpt::Config(ref cmd) => cmd,
            TEdgeOpt::Connect(ref cmd) => cmd,
            TEdgeOpt::Disconnect(cmd) => cmd,
            TEdgeOpt::Mqtt(ref cmd) => cmd,
        }
    }
}

impl CliOption for TEdgeOpt {
    fn build_command(&self, config: &TEdgeConfig) -> Result<Box<dyn Command>, ConfigError> {
        self.sub_option().build_command(config)
    }
}
