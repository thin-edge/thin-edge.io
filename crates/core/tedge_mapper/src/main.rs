use std::{fmt, path::PathBuf};

use crate::{
    az::mapper::AzureMapper, c8y::mapper::CumulocityMapper, collectd::mapper::CollectdMapper,
    core::component::TEdgeComponent,
};
use clap::Parser;
use flockfile::check_another_instance_is_not_running;
use tedge_config::DEFAULT_TEDGE_CONFIG_PATH;
use tedge_config::*;

mod az;
mod c8y;
mod collectd;
mod core;

fn lookup_component(component_name: &MapperName) -> Box<dyn TEdgeComponent> {
    match component_name {
        MapperName::Az => Box::new(AzureMapper::new()),
        MapperName::Collectd => Box::new(CollectdMapper::new()),
        MapperName::C8y => Box::new(CumulocityMapper::new()),
    }
}

#[derive(Debug, Parser)]
#[clap(
    name = clap::crate_name!(),
    version = clap::crate_version!(),
    about = clap::crate_description!()
)]
pub struct MapperOpt {
    #[clap(subcommand)]
    pub name: MapperName,

    /// Turn-on the debug log level.
    ///
    /// If off only reports ERROR, WARN, and INFO
    /// If on also reports DEBUG and TRACE
    #[clap(long, global = true)]
    pub debug: bool,

    /// Start the mapper with clean session off, subscribe to the topics, so that no messages are lost
    #[clap(short, long)]
    pub init: bool,

    /// Start the agent with clean session on, drop the previous session and subscriptions
    ///
    /// WARNING: All pending messages will be lost.
    #[clap(short, long)]
    pub clear: bool,

    /// Start the mapper from custom path
    ///
    /// WARNING: This is mostly used in testing.
    #[clap(long = "config-dir", default_value = DEFAULT_TEDGE_CONFIG_PATH)]
    pub config_dir: PathBuf,
}

#[derive(Debug, clap::Subcommand)]
pub enum MapperName {
    Az,
    C8y,
    Collectd,
}

impl fmt::Display for MapperName {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            MapperName::Az => write!(f, "tedge-mapper-az"),
            MapperName::C8y => write!(f, "tedge-mapper-c8y"),
            MapperName::Collectd => write!(f, "tedge-mapper-collectd"),
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mapper = MapperOpt::parse();
    tedge_utils::logging::initialise_tracing_subscriber(mapper.debug);

    let component = lookup_component(&mapper.name);

    let tedge_config_location =
        tedge_config::TEdgeConfigLocation::from_custom_root(&mapper.config_dir);
    let config = tedge_config::TEdgeConfigRepository::new(tedge_config_location.clone()).load()?;
    // Run only one instance of a mapper
    let _flock = check_another_instance_is_not_running(&mapper.name.to_string())?;

    if mapper.init {
        let mut mapper = CumulocityMapper::new();
        mapper.init_session().await
    } else if mapper.clear {
        let mut mapper = CumulocityMapper::new();
        mapper.clear_session().await
    } else {
        component.start(config).await
    }
}
