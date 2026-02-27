use camino::Utf8PathBuf;
use std::sync::Arc;
use tedge_config::TEdgeConfig;

use super::common::CloudBorrow;
use super::connect::ConnectError;
use super::log::MaybeFancy;
use crate::bridge::BridgeConfig;
use crate::bridge::BridgeLocation;
use crate::bridge::CommonMosquittoConfig;
use crate::bridge::TEDGE_BRIDGE_CONF_DIR_PATH;
use crate::command::Command;
use tedge_system_services::service_manager;
use tedge_system_services::SystemService;
use tedge_system_services::SystemServiceManager;

pub struct RefreshBridgesCmd {
    service_manager: Arc<dyn SystemServiceManager>,
}

#[async_trait::async_trait]
impl Command for RefreshBridgesCmd {
    fn description(&self) -> String {
        "Refresh all currently active mosquitto bridges (restarts mosquitto)".to_string()
    }

    async fn execute(&self, config: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        self.execute_unfancy(config).await.map_err(<_>::into)
    }
}

impl RefreshBridgesCmd {
    pub fn new(config: &TEdgeConfig) -> Result<Self, crate::ConfigError> {
        let service_manager = service_manager(config.root_dir())?;

        let cmd = Self { service_manager };

        Ok(cmd)
    }

    async fn execute_unfancy(&self, config: TEdgeConfig) -> anyhow::Result<()> {
        let clouds = established_bridges(&config).await;

        if clouds.is_empty() && !config.mqtt.bridge.built_in {
            eprintln!("No bridges to refresh.");
            return Ok(());
        }

        let common_mosquitto_config = CommonMosquittoConfig::from_tedge_config(&config);
        common_mosquitto_config.save(&config).await?;

        if !config.mqtt.bridge.built_in {
            for cloud in &clouds {
                eprintln!("Refreshing bridge {cloud}");

                let bridge_config = super::connect::bridge_config(&config, cloud).await?;
                refresh_bridge(&bridge_config, &config).await?;
            }
        }

        for cloud in possible_clouds(&config) {
            // (attempt to) reassert ownership of the certificate and key
            // This is necessary when upgrading from the mosquitto bridge to the built-in bridge
            if let Ok(bridge_config) = super::connect::bridge_config(&config, &cloud).await {
                super::connect::chown_certificate_and_key(&bridge_config).await;

                if bridge_config.bridge_location == BridgeLocation::BuiltIn
                    && clouds.contains(&cloud)
                {
                    eprintln!(
                    "Deleting mosquitto bridge configuration for {cloud} in favour of built-in bridge"
                );
                    super::connect::use_built_in_bridge(&config, &bridge_config).await?;
                }
            }
        }

        eprintln!("Restarting mosquitto service.\n");
        self.service_manager
            .restart_service(SystemService::new("mosquitto"))
            .await?;

        Ok(())
    }
}

async fn established_bridges(tedge_config: &TEdgeConfig) -> Vec<CloudBorrow<'_>> {
    // if the bridge configuration file doesn't exist, then the bridge doesn't exist and we shouldn't try to update it
    possible_clouds(tedge_config)
        .filter(|cloud| get_bridge_config_file_path_cloud(tedge_config, cloud).exists())
        .collect()
}

fn possible_clouds(config: &TEdgeConfig) -> impl Iterator<Item = CloudBorrow<'_>> {
    let iter = ::std::iter::empty();
    #[cfg(feature = "c8y")]
    let iter = iter.chain(config.c8y_keys().map(CloudBorrow::c8y_borrowed));
    #[cfg(feature = "azure")]
    let iter = iter.chain(config.az_keys().map(CloudBorrow::az_borrowed));
    #[cfg(feature = "aws")]
    let iter = iter.chain(config.aws_keys().map(CloudBorrow::aws_borrowed));

    iter
}

pub async fn refresh_bridge(
    bridge_config: &BridgeConfig,
    tedge_config: &TEdgeConfig,
) -> Result<(), ConnectError> {
    // if error, no need to clean up because the file already exists
    bridge_config.save(tedge_config).await?;

    Ok(())
}

pub fn get_bridge_config_file_path_cloud(
    tedge_config: &TEdgeConfig,
    cloud: &CloudBorrow<'_>,
) -> Utf8PathBuf {
    tedge_config
        .root_dir()
        .join(TEDGE_BRIDGE_CONF_DIR_PATH)
        .join(&*cloud.mosquitto_config_filename())
}
