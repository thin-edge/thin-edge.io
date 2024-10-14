use std::sync::Arc;

use camino::Utf8PathBuf;
use tedge_config::system_services::SystemService;
use tedge_config::system_services::SystemServiceManager;
use tedge_config::ProfileName;
use tedge_config::TEdgeConfig;
use tedge_config::TEdgeConfigLocation;

use super::common::Cloud;
use super::connect::ConnectError;
use crate::bridge::BridgeConfig;
use crate::bridge::BridgeLocation;
use crate::bridge::CommonMosquittoConfig;
use crate::bridge::TEDGE_BRIDGE_CONF_DIR_PATH;
use crate::command::BuildContext;
use crate::command::Command;

pub struct RefreshBridgesCmd {
    config: TEdgeConfig,
    config_location: TEdgeConfigLocation,
    service_manager: Arc<dyn SystemServiceManager>,
}

impl Command for RefreshBridgesCmd {
    fn description(&self) -> String {
        "Refresh all currently active mosquitto bridges (restarts mosquitto)".to_string()
    }

    fn execute(&self) -> anyhow::Result<()> {
        let clouds = established_bridges(&self.config_location, &self.config);

        if clouds.is_empty() && !self.config.mqtt.bridge.built_in {
            println!("No bridges to refresh.");
            return Ok(());
        }

        let common_mosquitto_config = CommonMosquittoConfig::from_tedge_config(&self.config);
        common_mosquitto_config.save(&self.config_location)?;

        if !self.config.mqtt.bridge.built_in {
            for (cloud, profile) in &clouds {
                println!("Refreshing bridge {cloud}");

                let bridge_config = super::connect::bridge_config(&self.config, *cloud, *profile)?;
                refresh_bridge(&bridge_config, &self.config_location)?;
            }
        }

        for (cloud, profile) in possible_clouds(&self.config) {
            // (attempt to) reassert ownership of the certificate and key
            // This is necessary when upgrading from the mosquitto bridge to the built-in bridge
            if let Ok(bridge_config) = super::connect::bridge_config(&self.config, cloud, profile) {
                super::connect::chown_certificate_and_key(&bridge_config);

                if bridge_config.bridge_location == BridgeLocation::BuiltIn
                    && clouds.contains(&(cloud, profile))
                {
                    println!(
                    "Deleting mosquitto bridge configuration for {cloud} in favour of built-in bridge"
                );
                    super::connect::use_built_in_bridge(&self.config_location, &bridge_config)?;
                }
            }
        }

        println!("Restarting mosquitto service.\n");
        self.service_manager
            .restart_service(SystemService::Mosquitto)?;

        Ok(())
    }
}

impl RefreshBridgesCmd {
    pub fn new(context: &BuildContext) -> Result<Self, crate::ConfigError> {
        let config = context.load_config()?;
        let config_location = context.config_location.clone();
        let service_manager = tedge_config::system_services::service_manager(
            &config_location.tedge_config_root_path,
        )?;

        let cmd = Self {
            config,
            config_location,
            service_manager,
        };

        Ok(cmd)
    }
}

fn established_bridges<'a>(
    config_location: &TEdgeConfigLocation,
    config: &'a TEdgeConfig,
) -> Vec<(Cloud, Option<&'a ProfileName>)> {
    // if the bridge configuration file doesn't exist, then the bridge doesn't exist and we shouldn't try to update it
    possible_clouds(config)
        .filter(|(cloud, profile)| {
            get_bridge_config_file_path_cloud(config_location, *cloud, *profile).exists()
        })
        .collect()
}

fn possible_clouds(config: &TEdgeConfig) -> impl Iterator<Item = (Cloud, Option<&ProfileName>)> {
    config
        .c8y
        .keys()
        .map(|profile| (Cloud::C8y, profile))
        .chain(config.az.keys().map(|profile| (Cloud::Azure, profile)))
        .chain(config.aws.keys().map(|profile| (Cloud::Aws, profile)))
}

pub fn refresh_bridge(
    bridge_config: &BridgeConfig,
    config_location: &TEdgeConfigLocation,
) -> Result<(), ConnectError> {
    // if error, no need to clean up because the file already exists
    bridge_config.save(config_location)?;

    Ok(())
}

pub fn get_bridge_config_file_path_cloud(
    config_location: &TEdgeConfigLocation,
    cloud: Cloud,
    profile: Option<&ProfileName>,
) -> Utf8PathBuf {
    config_location
        .tedge_config_root_path
        .join(TEDGE_BRIDGE_CONF_DIR_PATH)
        .join(&*cloud.bridge_config_filename(profile))
}
