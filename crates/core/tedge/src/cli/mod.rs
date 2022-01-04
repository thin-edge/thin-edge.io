use crate::command::{BuildCommand, BuildContext, Command};
use crate::system_services::{SystemConfigError, SystemServiceManager};
use crate::GeneralServiceManager;
use std::path::PathBuf;
use std::sync::Arc;
use structopt::clap;
use structopt::StructOpt;
use tedge_users::UserManager;

mod certificate;
mod config;
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
    Cert(certificate::TEdgeCertCli),

    /// Configure Thin Edge.
    Config(config::ConfigCmd),

    /// Connect to connector provider
    Connect(connect::TEdgeConnectOpt),

    /// Remove bridge connection for a provider
    Disconnect(disconnect::TEdgeDisconnectBridgeCli),

    /// Publish a message on a topic and subscribe a topic.
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

pub(crate) fn service_manager(
    user_manager: UserManager,
    config_root: PathBuf,
) -> Result<Arc<dyn SystemServiceManager>, SystemConfigError> {
    Ok(Arc::new(GeneralServiceManager::try_new(
        user_manager,
        config_root,
    )?))
}
