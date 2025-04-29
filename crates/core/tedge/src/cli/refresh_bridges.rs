use camino::Utf8PathBuf;
use std::sync::Arc;
use tedge_config::TEdgeConfig;
use tedge_config::TEdgeConfigLocation;

use super::common::CloudBorrow;
use super::connect::ConnectError;
use super::log::MaybeFancy;
use crate::bridge::BridgeConfig;
use crate::bridge::BridgeLocation;
use crate::bridge::CommonMosquittoConfig;
use crate::bridge::TEDGE_BRIDGE_CONF_DIR_PATH;
use crate::command::Command;
use crate::system_services::service_manager;
use crate::system_services::SystemService;
use crate::system_services::SystemServiceManager;

pub struct RefreshBridgesCmd {
    config: TEdgeConfig,
    config_location: TEdgeConfigLocation,
    service_manager: Arc<dyn SystemServiceManager>,
}

#[async_trait::async_trait]
impl Command for RefreshBridgesCmd {
    fn description(&self) -> String {
        "Refresh all currently active mosquitto bridges (restarts mosquitto)".to_string()
    }

    async fn execute(&self) -> Result<(), MaybeFancy<anyhow::Error>> {
        self.execute_unfancy().await.map_err(<_>::into)
    }
}

impl RefreshBridgesCmd {
    pub fn new(
        config: TEdgeConfig,
        config_location: TEdgeConfigLocation,
    ) -> Result<Self, crate::ConfigError> {
        let service_manager = service_manager(&config_location.tedge_config_root_path)?;

        let cmd = Self {
            config,
            config_location,
            service_manager,
        };

        Ok(cmd)
    }

    async fn execute_unfancy(&self) -> anyhow::Result<()> {
        let clouds = established_bridges(&self.config_location, &self.config).await;

        if clouds.is_empty() && !self.config.mqtt.bridge.built_in {
            println!("No bridges to refresh.");
            return Ok(());
        }

        let common_mosquitto_config = CommonMosquittoConfig::from_tedge_config(&self.config);
        common_mosquitto_config.save(&self.config_location).await?;

        if !self.config.mqtt.bridge.built_in {
            for cloud in &clouds {
                println!("Refreshing bridge {cloud}");

                let bridge_config = super::connect::bridge_config(&self.config, cloud)?;
                refresh_bridge(&bridge_config, &self.config_location).await?;
            }
        }

        for cloud in possible_clouds(&self.config) {
            // (attempt to) reassert ownership of the certificate and key
            // This is necessary when upgrading from the mosquitto bridge to the built-in bridge
            if let Ok(bridge_config) = super::connect::bridge_config(&self.config, &cloud) {
                super::connect::chown_certificate_and_key(&bridge_config).await;

                if bridge_config.bridge_location == BridgeLocation::BuiltIn
                    && clouds.contains(&cloud)
                {
                    println!(
                    "Deleting mosquitto bridge configuration for {cloud} in favour of built-in bridge"
                );
                    super::connect::use_built_in_bridge(&self.config_location, &bridge_config)
                        .await?;
                }
            }
        }

        println!("Restarting mosquitto service.\n");
        self.service_manager
            .restart_service(SystemService::Mosquitto)
            .await?;

        Ok(())
    }
}

async fn established_bridges<'a>(
    config_location: &TEdgeConfigLocation,
    config: &'a TEdgeConfig,
) -> Vec<CloudBorrow<'a>> {
    // if the bridge configuration file doesn't exist, then the bridge doesn't exist and we shouldn't try to update it
    possible_clouds(config)
        .filter(|cloud| get_bridge_config_file_path_cloud(config_location, cloud).exists())
        .collect()
}

fn possible_clouds(config: &TEdgeConfig) -> impl Iterator<Item = CloudBorrow<'_>> {
    let iter = ::std::iter::empty();
    #[cfg(feature = "c8y")]
    let iter = iter.chain(config.c8y.keys().map(CloudBorrow::c8y_borrowed));
    #[cfg(feature = "azure")]
    let iter = iter.chain(config.az.keys().map(CloudBorrow::az_borrowed));
    #[cfg(feature = "aws")]
    let iter = iter.chain(config.aws.keys().map(CloudBorrow::aws_borrowed));

    iter
}

pub async fn refresh_bridge(
    bridge_config: &BridgeConfig,
    config_location: &TEdgeConfigLocation,
) -> Result<(), ConnectError> {
    // if error, no need to clean up because the file already exists
    bridge_config.save(config_location).await?;

    Ok(())
}

pub fn get_bridge_config_file_path_cloud(
    config_location: &TEdgeConfigLocation,
    cloud: &CloudBorrow<'_>,
) -> Utf8PathBuf {
    config_location
        .tedge_config_root_path
        .join(TEDGE_BRIDGE_CONF_DIR_PATH)
        .join(&*cloud.bridge_config_filename())
}
