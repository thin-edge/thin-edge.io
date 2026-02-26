//! Bridge configuration persistence utilities.
//!
//! This module provides functions for persisting and loading bridge configuration
//! files using a three-file pattern that supports user customization.

use anyhow::Context;
use camino::Utf8Path;
use tedge_config::tedge_toml::ProfileName;
use tedge_config::TEdgeConfig;
use tedge_utils::file::change_mode;
use tedge_utils::file::change_user_and_group;
use tedge_utils::file::{self};
use tedge_utils::fs;
use tracing::warn;

use crate::config::expand_bridge_rules;
use crate::config::BridgeConfig;
use crate::config_toml::AuthMethod;
use crate::config_toml::ExpandedBridgeRule;
use crate::config_toml::NonExpansionReason;

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
    let prior_config = tokio::fs::read(&config_path).await.ok();
    let prior_template = tokio::fs::read(&template_path).await.ok();
    let overridden = prior_config != prior_template;
    let disabled = tokio::fs::try_exists(&disabled_config_path)
        .await
        .unwrap_or(false);
    let update_flow = !overridden && !disabled;

    // Persist a copy of bridge config definition to be used by users as a template
    file::create_directory_with_defaults(dir).await?;
    fs::atomically_write_file_async(&template_path, content.as_bytes()).await?;

    if let Err(err) = change_user_and_group(&template_path, "tedge", "tedge").await {
        warn!("failed to set file ownership for '{template_path}': {err}");
    }

    if let Err(err) = change_mode(&template_path, 0o644).await {
        warn!("failed to set file permissions for '{template_path}': {err}");
    }

    if update_flow {
        fs::atomically_write_file_async(&config_path, content.as_bytes()).await?;
        if let Err(err) = change_user_and_group(&config_path, "tedge", "tedge").await {
            warn!("failed to set file ownership for '{config_path}': {err}");
        }

        if let Err(err) = change_mode(&config_path, 0o644).await {
            warn!("failed to set file permissions for '{config_path}': {err}");
        }
    }

    Ok(())
}

/// Trait for processing bridge configuration files discovered during directory traversal.
///
/// Implementors receive callbacks as `.toml` files are read and expanded.
pub trait BridgeConfigVisitor {
    /// Called for each `.toml` file that has a `.toml.disabled` marker.
    fn on_file_disabled(&mut self, _path: &Utf8Path) {}

    /// Called with the expanded rules from each enabled `.toml` file.
    fn on_rules_loaded(
        &mut self,
        path: &Utf8Path,
        source: &str,
        rules: Vec<ExpandedBridgeRule>,
        non_expansions: Vec<NonExpansionReason>,
    ) -> anyhow::Result<()>;
}

/// Walks a bridge configuration directory, expanding each `.toml` file
/// and notifying the visitor of results.
///
/// Files with non-`.toml` extensions (like `.toml.template`) are ignored.
/// Files with a `.toml.disabled` marker trigger [`BridgeConfigVisitor::on_file_disabled`].
/// All other `.toml` files are read, expanded, and passed to
/// [`BridgeConfigVisitor::on_rules_loaded`].
pub async fn visit_bridge_config_dir(
    dir: &Utf8Path,
    tedge_config: &TEdgeConfig,
    auth_method: AuthMethod,
    cloud_profile: Option<&ProfileName>,
    mapper_config: Option<&toml::Table>,
    visitor: &mut impl BridgeConfigVisitor,
) -> anyhow::Result<()> {
    let mut read_dir = tokio::fs::read_dir(dir)
        .await
        .with_context(|| format!("failed to read bridge config directory: {dir}"))?;

    while let Some(entry) = read_dir.next_entry().await? {
        let path = entry.path();
        let Some(utf8_path) = path.to_str().map(Utf8Path::new) else {
            continue;
        };

        if utf8_path.extension() != Some("toml") {
            continue;
        }

        let disabled_path = utf8_path.with_extension("toml.disabled");
        if tokio::fs::try_exists(&disabled_path).await.unwrap_or(false) {
            visitor.on_file_disabled(utf8_path);
            continue;
        }

        let content = tokio::fs::read_to_string(utf8_path)
            .await
            .with_context(|| format!("failed to read {utf8_path}"))?;

        let (rules, non_expansions) = expand_bridge_rules(
            utf8_path,
            &content,
            tedge_config,
            auth_method,
            cloud_profile,
            mapper_config,
        )?;

        visitor.on_rules_loaded(utf8_path, &content, rules, non_expansions)?;
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
    mapper_config: Option<&toml::Table>,
) -> anyhow::Result<BridgeConfig> {
    struct RuntimeVisitor(BridgeConfig);

    impl BridgeConfigVisitor for RuntimeVisitor {
        fn on_rules_loaded(
            &mut self,
            _path: &Utf8Path,
            _source: &str,
            rules: Vec<ExpandedBridgeRule>,
            _non_expansions: Vec<NonExpansionReason>,
        ) -> anyhow::Result<()> {
            self.0.add_expanded_rules(rules)?;
            Ok(())
        }
    }

    let mut visitor = RuntimeVisitor(BridgeConfig::new());
    visit_bridge_config_dir(
        dir,
        tedge_config,
        auth_method,
        cloud_profile,
        mapper_config,
        &mut visitor,
    )
    .await?;
    Ok(visitor.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tedge_test_utils::fs::TempTedgeDir;

    mod persist_bridge_config_file {
        use super::*;

        #[tokio::test]
        async fn creates_both_config_and_template_when_neither_exists() {
            let ttd = TempTedgeDir::new();
            let dir = ttd.utf8_path().join("bridge");
            tokio::fs::create_dir_all(&dir).await.unwrap();
            let content = "test content";

            persist_bridge_config_file(&dir, "test", content)
                .await
                .unwrap();

            assert_eq!(
                tokio::fs::read_to_string(dir.join("test.toml"))
                    .await
                    .unwrap(),
                content
            );
            assert_eq!(
                tokio::fs::read_to_string(dir.join("test.toml.template"))
                    .await
                    .unwrap(),
                content
            );
        }

        #[tokio::test]
        async fn updates_both_when_config_matches_template() {
            let ttd = TempTedgeDir::new();
            let dir = ttd.utf8_path().join("bridge");
            tokio::fs::create_dir_all(&dir).await.unwrap();

            // Set up matching config and template
            let old_content = "old content";
            tokio::fs::write(dir.join("test.toml"), old_content)
                .await
                .unwrap();
            tokio::fs::write(dir.join("test.toml.template"), old_content)
                .await
                .unwrap();

            let new_content = "new content";
            persist_bridge_config_file(&dir, "test", new_content)
                .await
                .unwrap();

            assert_eq!(
                tokio::fs::read_to_string(dir.join("test.toml"))
                    .await
                    .unwrap(),
                new_content
            );
            assert_eq!(
                tokio::fs::read_to_string(dir.join("test.toml.template"))
                    .await
                    .unwrap(),
                new_content
            );
        }

        #[tokio::test]
        async fn only_updates_template_when_config_is_overridden() {
            let ttd = TempTedgeDir::new();
            let dir = ttd.utf8_path().join("bridge");
            tokio::fs::create_dir_all(&dir).await.unwrap();

            // Set up differing config and template (user has customized)
            let custom_config = "custom user config";
            let old_template = "old template";
            tokio::fs::write(dir.join("test.toml"), custom_config)
                .await
                .unwrap();
            tokio::fs::write(dir.join("test.toml.template"), old_template)
                .await
                .unwrap();

            let new_content = "new content";
            persist_bridge_config_file(&dir, "test", new_content)
                .await
                .unwrap();

            // Config should remain unchanged
            assert_eq!(
                tokio::fs::read_to_string(dir.join("test.toml"))
                    .await
                    .unwrap(),
                custom_config
            );
            // Template should be updated
            assert_eq!(
                tokio::fs::read_to_string(dir.join("test.toml.template"))
                    .await
                    .unwrap(),
                new_content
            );
        }

        #[tokio::test]
        async fn only_updates_template_when_disabled_marker_exists() {
            let ttd = TempTedgeDir::new();
            let dir = ttd.utf8_path().join("bridge");
            tokio::fs::create_dir_all(&dir).await.unwrap();

            // Set up matching config and template with disabled marker
            let old_content = "old content";
            tokio::fs::write(dir.join("test.toml"), old_content)
                .await
                .unwrap();
            tokio::fs::write(dir.join("test.toml.template"), old_content)
                .await
                .unwrap();
            tokio::fs::write(dir.join("test.toml.disabled"), "")
                .await
                .unwrap();

            let new_content = "new content";
            persist_bridge_config_file(&dir, "test", new_content)
                .await
                .unwrap();

            // Config should remain unchanged because disabled
            assert_eq!(
                tokio::fs::read_to_string(dir.join("test.toml"))
                    .await
                    .unwrap(),
                old_content
            );
            // Template should be updated
            assert_eq!(
                tokio::fs::read_to_string(dir.join("test.toml.template"))
                    .await
                    .unwrap(),
                new_content
            );
        }
    }

    mod load_bridge_rules_from_directory {
        use super::*;

        #[tokio::test]
        async fn skips_disabled_config_files() {
            let ttd = TempTedgeDir::new();
            let bridge_dir = ttd.dir("mappers").dir("c8y").dir("bridge");
            let bridge_dir = bridge_dir.utf8_path();

            // Create valid bridge configs
            let config_content = r#"
                local_prefix = "local/"
                remote_prefix = "remote/"

                [[rule]]
                topic = "test/topic"
                direction = "inbound"
            "#;
            tokio::fs::write(bridge_dir.join("enabled.toml"), config_content)
                .await
                .unwrap();
            tokio::fs::write(bridge_dir.join("disabled.toml"), config_content)
                .await
                .unwrap();
            // Mark the second config as disabled
            tokio::fs::write(bridge_dir.join("disabled.toml.disabled"), "")
                .await
                .unwrap();

            let tedge_config = TEdgeConfig::load(ttd.utf8_path()).await.unwrap();

            let bridge_config = load_bridge_rules_from_directory(
                bridge_dir,
                &tedge_config,
                AuthMethod::Certificate,
                None,
                None,
            )
            .await
            .unwrap();

            // Should only have rules from enabled.toml (1 rule), not disabled.toml
            assert_eq!(bridge_config.remote_subscriptions().count(), 1);
        }

        #[tokio::test]
        async fn ignores_template_files() {
            let ttd = TempTedgeDir::new();
            let bridge_dir = ttd.dir("mappers").dir("c8y").dir("bridge");
            let bridge_dir = bridge_dir.utf8_path();

            // Create a valid bridge config and its template
            let config_content = r#"
                local_prefix = "local/"
                remote_prefix = "remote/"

                [[rule]]
                topic = "test/topic"
                direction = "inbound"
            "#;
            tokio::fs::write(bridge_dir.join("config.toml"), config_content)
                .await
                .unwrap();
            tokio::fs::write(bridge_dir.join("config.toml.template"), config_content)
                .await
                .unwrap();

            let tedge_config = TEdgeConfig::load(ttd.utf8_path()).await.unwrap();

            let bridge_config = load_bridge_rules_from_directory(
                bridge_dir,
                &tedge_config,
                AuthMethod::Certificate,
                None,
                None,
            )
            .await
            .unwrap();

            // Should only load config.toml, not config.toml.template
            // So we should have 1 rule, not 2
            assert_eq!(bridge_config.remote_subscriptions().count(), 1);
        }
    }
}
