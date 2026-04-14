//! Creating and updating `mosquitto.conf` files for MQTT bridges to different clouds.

mod common_mosquitto_config;
mod config;

use tedge_config::TEdgeConfig;
use tedge_utils::paths::Owner;
use tedge_utils::paths::PathsError;

#[cfg(feature = "aws")]
pub mod aws;
#[cfg(feature = "azure")]
pub mod azure;
#[cfg(feature = "c8y")]
pub mod c8y;

pub use common_mosquitto_config::*;
pub use config::BridgeConfig;
pub use config::BridgeLocation;

pub const TEDGE_BRIDGE_CONF_DIR_PATH: &str = "mosquitto-conf";

pub(crate) async fn write_mosquitto_owned_config(
    tedge_config: &TEdgeConfig,
    config_file: &str,
    contents: &[u8],
) -> Result<(), PathsError> {
    let config_root = tedge_config.config_root();
    config_root
        .dir(TEDGE_BRIDGE_CONF_DIR_PATH)?
        .with_mode(0o755)
        .ensure()
        .await?;
    config_root
        .file(format!("{TEDGE_BRIDGE_CONF_DIR_PATH}/{config_file}"))?
        .with_owner(Owner::user_group(crate::BROKER_USER, crate::BROKER_GROUP))
        .replace_atomic(contents)
        .await?;
    Ok(())
}
