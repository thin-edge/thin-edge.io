//! Creating and updating `mosquitto.conf` files for MQTT bridges to different clouds.

mod common_mosquitto_config;
mod config;

use tedge_config::TEdgeConfig;
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

pub(crate) async fn write_mosquitto_config(
    tedge_config: &TEdgeConfig,
    config_file: &str,
    contents: &[u8],
) -> Result<(), PathsError> {
    let config_root = tedge_config.config_root();
    let config_file = config_root.file(format!("{TEDGE_BRIDGE_CONF_DIR_PATH}/{config_file}"))?;
    config_file.parent().ensure().await?;
    config_file
        .use_process_ownership()
        .replace_atomic(contents)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::MetadataExt;
    use std::os::unix::fs::PermissionsExt;
    use tedge_config::TEdgeConfig;
    use tedge_test_utils::fs::TempTedgeDir;

    #[tokio::test]
    async fn mosquitto_config_files_keep_writer_ownership() {
        let ttd = TempTedgeDir::new();
        ttd.file("system.toml")
            .with_raw_content("user = ''\ngroup = ''\n");
        let config = TEdgeConfig::load(ttd.path()).await.unwrap();

        write_mosquitto_config(&config, "tedge-mosquitto.conf", b"listener 1883")
            .await
            .unwrap();

        let metadata = tokio::fs::metadata(
            ttd.path()
                .join(TEDGE_BRIDGE_CONF_DIR_PATH)
                .join("tedge-mosquitto.conf"),
        )
        .await
        .unwrap();
        assert_eq!(metadata.uid(), nix::unistd::geteuid().as_raw());
        assert_eq!(metadata.gid(), nix::unistd::getegid().as_raw());
        assert_eq!(metadata.permissions().mode() & 0o777, 0o644);
    }
}
