//! Bridge configuration persistence utilities.
//!
//! This module provides functions for persisting and loading bridge configuration
//! files using a three-file pattern that supports user customization.

use anyhow::Context;
use camino::Utf8Path;
use tedge_config::tedge_toml::ProfileName;
use tedge_config::TEdgeConfig;
use tedge_utils::file;
use tedge_utils::fs;

use crate::config::BridgeConfig;
use crate::config_toml::AuthMethod;

/// Persists a bridge config file using the three-file pattern:
///
/// - `{name}.toml` - active config (updated only if not overridden or disabled)
/// - `{name}.toml.template` - canonical template (always updated)
/// - `{name}.toml.disabled` - marker file indicating the config is disabled
///
/// This pattern allows users to customize their bridge configuration while still
/// receiving template updates. If a user has modified the `.toml` file (making it
/// differ from the `.toml.template`), or if a `.toml.disabled` file exists, the
/// active config will not be updated.
pub async fn persist_bridge_config_file(
    dir: &Utf8Path,
    name: &str,
    content: &str,
) -> anyhow::Result<()> {
    let config_path = dir.join(name).with_extension("toml");
    let disabled_config_path = dir.join(name).with_extension("toml.disabled");
    let template_path = dir.join(name).with_extension("toml.template");

    // Don't update the flow definition if overridden or disabled
    let prior_flow = tokio::fs::read(&config_path).await.ok();
    let prior_template = tokio::fs::read(&template_path).await.ok();
    let overridden = prior_flow != prior_template;
    let disabled = tokio::fs::try_exists(&disabled_config_path)
        .await
        .unwrap_or(false);
    let update_flow = !overridden && !disabled;

    // Persist a copy of bridge config definition to be used by users as a template
    file::create_directory_with_defaults(dir).await?;
    fs::atomically_write_file_async(template_path, content.as_bytes()).await?;

    if update_flow {
        fs::atomically_write_file_async(config_path, content.as_bytes()).await?;
    }

    Ok(())
}

/// Loads all bridge rules from `.toml` files in the specified directory.
///
/// This function reads all `.toml` files in the given directory and expands
/// them using the provided configuration context. Files with other extensions
/// (like `.toml.template` or `.toml.disabled`) are ignored.
pub async fn load_bridge_rules_from_directory(
    dir: &Utf8Path,
    tedge_config: &TEdgeConfig,
    auth_method: AuthMethod,
    cloud_profile: Option<&ProfileName>,
) -> anyhow::Result<BridgeConfig> {
    let mut tc = BridgeConfig::new();
    let mut read_dir = tokio::fs::read_dir(dir)
        .await
        .with_context(|| format!("failed to read bridge config directory: {dir}"))?;

    while let Some(file) = read_dir.next_entry().await? {
        let path = file.path();
        let Some(utf8_path) = path.to_str().map(Utf8Path::new) else {
            continue;
        };

        if utf8_path.extension() == Some("toml") {
            tc.add_rules_from_template(
                utf8_path,
                &tokio::fs::read_to_string(file.path())
                    .await
                    .with_context(|| format!("failed to read {utf8_path}"))?,
                tedge_config,
                auth_method,
                cloud_profile,
            )?;
        }
    }
    Ok(tc)
}
