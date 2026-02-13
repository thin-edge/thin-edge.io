use camino::Utf8Path;
use log::error;
use log::warn;
use serde::Deserialize;
use std::borrow::Borrow;
use std::collections::HashSet;
use std::fs;
use std::hash::Hash;
use std::hash::Hasher;
use tedge_config::SudoCommandBuilder;
use tedge_utils::file::PermissionEntry;

/// Valid service actions in the context of configuration file management.
/// Only the actions like and restart (and reload in future) are supported.
/// Other system.toml actions like "start", "stop", "enable", "disable", "is_active"
/// are intentionally not supported as they don't make sense in the context of config file changes.
const VALID_SERVICE_ACTIONS: &[&str] = &["restart"];

pub const DEFAULT_PLUGIN_CONFIG_TYPE: &str = "tedge-configuration-plugin";

#[derive(Deserialize, Debug, Default)]
#[serde(deny_unknown_fields)]
struct RawPluginConfig {
    pub files: Vec<RawFileEntry>,
}

#[derive(Deserialize, Debug, Default, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RawFileEntry {
    pub path: String,
    #[serde(rename = "type")]
    config_type: Option<String>,
    user: Option<String>,
    group: Option<String>,
    mode: Option<u32>,
    parent_user: Option<String>,
    parent_group: Option<String>,
    parent_mode: Option<u32>,
    pub service: Option<String>,
    pub service_action: Option<String>,
}

#[derive(Debug, Eq, PartialEq, Default, Clone)]
pub struct PluginConfig {
    pub files: HashSet<FileEntry>,
}

#[derive(Debug, Eq, Default, Clone)]
pub struct FileEntry {
    pub path: String,
    pub config_type: String,
    pub file_permissions: PermissionEntry,
    pub parent_permissions: PermissionEntry,
    pub service: Option<String>,
    pub service_action: Option<String>,
}

impl Hash for FileEntry {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.config_type.hash(state);
    }
}

impl PartialEq for FileEntry {
    fn eq(&self, other: &Self) -> bool {
        self.config_type == other.config_type
    }
}

impl Borrow<str> for FileEntry {
    fn borrow(&self) -> &str {
        &self.config_type
    }
}

impl FileEntry {
    pub fn new(
        path: impl Into<String>,
        config_type: impl Into<String>,
        file_permissions: PermissionEntry,
        parent_permissions: PermissionEntry,
        service: Option<String>,
        service_action: Option<String>,
    ) -> Self {
        let parent_user = parent_permissions
            .user
            .or_else(|| file_permissions.user.clone());
        let parent_group = parent_permissions
            .group
            .or_else(|| file_permissions.group.clone());

        Self {
            path: path.into(),
            config_type: config_type.into(),
            file_permissions,
            parent_permissions: PermissionEntry {
                user: parent_user,
                group: parent_group,
                mode: parent_permissions.mode,
            },
            service,
            service_action,
        }
    }
}

impl RawPluginConfig {
    pub fn new(config_file_path: &Utf8Path) -> Self {
        Self::read_config(config_file_path)
    }

    pub fn read_config(path: &Utf8Path) -> Self {
        let path_str = path.to_string();
        match fs::read_to_string(path) {
            Ok(contents) => match toml::from_str(contents.as_str()) {
                Ok(config) => config,
                _ => {
                    error!("The config file {} is malformed.", path_str);
                    Self::default()
                }
            },
            Err(_) => {
                warn!(
                    "The config file {} does not exist or is not readable.",
                    path_str
                );
                Self::default()
            }
        }
    }
}

impl PluginConfig {
    pub fn new(config_file_path: &Utf8Path) -> Self {
        let plugin_config = Self::new_with_config_file_entry(config_file_path);
        let raw_config = RawPluginConfig::new(config_file_path);
        plugin_config.add_entries_from_raw_config(raw_config)
    }

    fn new_with_config_file_entry(config_file_path: &Utf8Path) -> Self {
        let file_entry = FileEntry::new(
            config_file_path.to_string(),
            DEFAULT_PLUGIN_CONFIG_TYPE,
            PermissionEntry::default(),
            PermissionEntry::default(),
            None,
            None,
        );
        Self {
            files: HashSet::from([file_entry]),
        }
    }

    fn add_entries_from_raw_config(mut self, raw_config: RawPluginConfig) -> Self {
        let original_plugin_config = self.clone();
        for raw_entry in raw_config.files {
            let config_type = raw_entry
                .config_type
                .unwrap_or_else(|| raw_entry.path.clone());

            if config_type.contains(['+', '#']) {
                error!(
                    "The config type '{}' contains the forbidden characters, '+' or '#'.",
                    config_type
                );
                return original_plugin_config;
            }

            let service_action = match (&raw_entry.service, &raw_entry.service_action) {
                (Some(_), Some(action)) => {
                    if !VALID_SERVICE_ACTIONS.contains(&action.as_str()) {
                        error!(
                            "The service_action '{}' is not supported. Valid actions are: {}",
                            action,
                            VALID_SERVICE_ACTIONS.join(", ")
                        );
                        return original_plugin_config;
                    }
                    Some(action.clone())
                }
                // Service provided but no action - default to "restart"
                (Some(_), None) => Some("restart".to_string()),
                // Action provided but no service - error
                (None, Some(_)) => {
                    error!(
                        "The service_action field requires a service field to be specified for config type '{}'.",
                        config_type
                    );
                    return original_plugin_config;
                }
                (None, None) => None,
            };

            let entry = FileEntry::new(
                raw_entry.path,
                config_type.clone(),
                PermissionEntry::new(raw_entry.user, raw_entry.group, raw_entry.mode),
                PermissionEntry::new(
                    raw_entry.parent_user,
                    raw_entry.parent_group,
                    raw_entry.parent_mode,
                ),
                raw_entry.service,
                service_action,
            );

            if !self.files.insert(entry) {
                error!("The config file has the duplicated type '{}'.", config_type);
                return original_plugin_config;
            }
        }
        self
    }

    pub fn get_file_entry(&self, config_type: &str) -> Option<&FileEntry> {
        self.files.get(config_type)
    }

    pub fn get_all_file_types(&self) -> Vec<String> {
        self.files
            .iter()
            .map(|x| x.config_type.to_string())
            .collect::<Vec<_>>()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TedgeWriteStatus {
    Enabled { sudo: SudoCommandBuilder },
    Disabled,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tedge_test_utils::fs::TempTedgeDir;

    #[test]
    fn test_get_all_file_types() {
        let ttd = TempTedgeDir::new();
        let toml_content = r#"
[[files]]
path = "/etc/app.conf"
type = "app.conf"

[[files]]
path = "/etc/service.toml"
type = "service.toml"

[[files]]
path = "/etc/nginx.conf"
type = "nginx.conf"
"#;
        let config_file = ttd
            .file("plugin_config.toml")
            .with_raw_content(toml_content);
        let config = PluginConfig::new(config_file.utf8_path());

        let types = config.get_all_file_types();

        assert_eq!(types.len(), 4); // Includes the plugin config itself
        assert!(types.contains(&"app.conf".to_string()));
        assert!(types.contains(&"service.toml".to_string()));
        assert!(types.contains(&"nginx.conf".to_string()));
        assert!(types.contains(&DEFAULT_PLUGIN_CONFIG_TYPE.to_string()));
    }

    #[test]
    fn test_get_all_file_types_returns_empty_for_empty_config() {
        let ttd = TempTedgeDir::new();
        let toml_content = r#""#;
        let config_file = ttd
            .file("plugin_config.toml")
            .with_raw_content(toml_content);
        let config = PluginConfig::new(config_file.utf8_path());

        let types = config.get_all_file_types();

        assert_eq!(types.len(), 1); // Only the plugin config itself
        assert!(types.contains(&DEFAULT_PLUGIN_CONFIG_TYPE.to_string()));
    }

    #[test]
    fn test_get_all_file_types_returns_single_type() {
        let ttd = TempTedgeDir::new();
        let toml_content = r#"
[[files]]
path = "/etc/single.conf"
type = "single.conf"
"#;
        let config_file = ttd
            .file("plugin_config.toml")
            .with_raw_content(toml_content);
        let config = PluginConfig::new(config_file.utf8_path());

        let types = config.get_all_file_types();

        assert_eq!(types.len(), 2); // Includes the plugin config itself
        assert!(types.contains(&"single.conf".to_string()));
    }

    #[test]
    fn test_get_all_file_types_only_returns_types_not_paths() {
        let ttd = TempTedgeDir::new();
        let toml_content = r#"
[[files]]
path = "/etc/path1/app.conf"
type = "app.conf"

[[files]]
path = "/etc/path2/app.conf"
type = "other.conf"
"#;
        let config_file = ttd
            .file("plugin_config.toml")
            .with_raw_content(toml_content);
        let config = PluginConfig::new(config_file.utf8_path());

        let types = config.get_all_file_types();

        assert_eq!(types.len(), 3); // Includes the plugin config itself
                                    // Should return config types, not file paths
        assert!(types.contains(&"app.conf".to_string()));
        assert!(types.contains(&"other.conf".to_string()));
        assert!(!types.contains(&"/etc/path1/app.conf".to_string()));
        assert!(!types.contains(&"/etc/path2/app.conf".to_string()));
    }

    #[test]
    fn test_file_entry_new_inherits_parent_permissions() {
        let file_perms = PermissionEntry {
            user: Some("tedge".to_string()),
            group: Some("tedge".to_string()),
            mode: Some(0o644),
        };
        let parent_perms = PermissionEntry::default();

        let entry = FileEntry::new(
            "/etc/test.conf",
            "test.conf",
            file_perms,
            parent_perms,
            None,
            None,
        );

        // Parent should inherit user and group from file permissions
        assert_eq!(entry.parent_permissions.user, Some("tedge".to_string()));
        assert_eq!(entry.parent_permissions.group, Some("tedge".to_string()));
        assert_eq!(entry.parent_permissions.mode, None);
    }

    #[test]
    fn test_file_entry_new_respects_explicit_parent_permissions() {
        let file_perms = PermissionEntry {
            user: Some("user1".to_string()),
            group: Some("group1".to_string()),
            mode: Some(0o644),
        };
        let parent_perms = PermissionEntry {
            user: Some("parent_user".to_string()),
            group: Some("parent_group".to_string()),
            mode: Some(0o755),
        };

        let entry = FileEntry::new(
            "/etc/test.conf",
            "test.conf",
            file_perms,
            parent_perms,
            None,
            None,
        );

        // Parent should use its own explicit permissions, not inherit
        assert_eq!(
            entry.parent_permissions.user,
            Some("parent_user".to_string())
        );
        assert_eq!(
            entry.parent_permissions.group,
            Some("parent_group".to_string())
        );
        assert_eq!(entry.parent_permissions.mode, Some(0o755));
    }

    #[test]
    fn test_plugin_config_get_file_entry() {
        let ttd = TempTedgeDir::new();
        let toml_content = r#"
[[files]]
path = "/etc/app.conf"
type = "app.conf"

[[files]]
path = "/etc/service.toml"
type = "service.toml"
"#;
        let config_file = ttd
            .file("plugin_config.toml")
            .with_raw_content(toml_content);
        let config = PluginConfig::new(config_file.utf8_path());

        assert!(config.get_file_entry("app.conf").is_some());
        assert!(config.get_file_entry("service.toml").is_some());
        assert!(config.get_file_entry("unknown.conf").is_none());

        let retrieved = config.get_file_entry("app.conf").unwrap();
        assert_eq!(retrieved.path, "/etc/app.conf");
    }

    #[test]
    fn test_plugin_config_rejects_forbidden_plus_character_in_type() {
        let ttd = TempTedgeDir::new();

        let toml_content = r#"
[[files]]
path = "/etc/test.conf"
type = "test+config"
"#;
        let config_file = ttd.file("forbidden.toml").with_raw_content(toml_content);

        let config = PluginConfig::new(config_file.utf8_path());

        // Should only contain the plugin config itself, not the invalid entry
        assert_eq!(config.files.len(), 1);
        assert!(config.get_file_entry(DEFAULT_PLUGIN_CONFIG_TYPE).is_some());
        assert!(config.get_file_entry("test+config").is_none());
    }

    #[test]
    fn test_plugin_config_rejects_forbidden_hash_character_in_type() {
        let ttd = TempTedgeDir::new();

        let toml_content = r#"
[[files]]
path = "/etc/test.conf"
type = "test#config"
"#;
        let config_file = ttd
            .file("forbidden_hash.toml")
            .with_raw_content(toml_content);

        let config = PluginConfig::new(config_file.utf8_path());

        // Should only contain the plugin config itself, not the invalid entry
        assert_eq!(config.files.len(), 1);
        assert!(config.get_file_entry("test#config").is_none());
    }

    #[test]
    fn test_plugin_config_rejects_duplicate_types() {
        let ttd = TempTedgeDir::new();

        let toml_content = r#"
[[files]]
path = "/etc/test1.conf"
type = "duplicate.conf"

[[files]]
path = "/etc/test2.conf"
type = "duplicate.conf"
"#;
        let config_file = ttd.file("duplicate.toml").with_raw_content(toml_content);

        let config = PluginConfig::new(config_file.utf8_path());

        // Should only contain the plugin config itself, duplicates rejected
        assert_eq!(config.files.len(), 1);
        assert!(config.get_file_entry(DEFAULT_PLUGIN_CONFIG_TYPE).is_some());
        assert!(config.get_file_entry("duplicate.conf").is_none());
    }

    #[test]
    fn test_plugin_config_uses_path_as_type_when_not_specified() {
        let ttd = TempTedgeDir::new();

        let toml_content = r#"
[[files]]
path = "/etc/myapp.conf"
"#;
        let config_file = ttd.file("no_type.toml").with_raw_content(toml_content);

        let config = PluginConfig::new(config_file.utf8_path());

        // Should use path as config_type when type is not specified
        assert!(config.get_file_entry("/etc/myapp.conf").is_some());
        let entry = config.get_file_entry("/etc/myapp.conf").unwrap();
        assert_eq!(entry.path, "/etc/myapp.conf");
    }

    #[test]
    fn test_plugin_config_accepts_restart_action() {
        let ttd = TempTedgeDir::new();

        let toml_content = r#"
[[files]]
path = "/etc/test.conf"
type = "test.conf"
service = "testservice"
service_action = "restart"
"#;
        let config_file = ttd
            .file("valid_action_restart.toml")
            .with_raw_content(toml_content);

        let config = PluginConfig::new(config_file.utf8_path());

        assert_eq!(config.files.len(), 2);
        let entry = config.get_file_entry("test.conf").unwrap();
        assert_eq!(entry.service, Some("testservice".to_string()));
        assert_eq!(entry.service_action, Some("restart".to_string()));
    }

    #[test]
    fn test_plugin_config_rejects_invalid_service_action() {
        let ttd = TempTedgeDir::new();

        let toml_content = r#"
[[files]]
path = "/etc/test.conf"
type = "test.conf"
service = "testservice"
service_action = "start"
"#;
        let config_file = ttd
            .file("invalid_action.toml")
            .with_raw_content(toml_content);

        let config = PluginConfig::new(config_file.utf8_path());

        // Should only contain the plugin config itself, invalid service_action rejected
        assert_eq!(config.files.len(), 1);
        assert!(config.get_file_entry(DEFAULT_PLUGIN_CONFIG_TYPE).is_some());
        assert!(config.get_file_entry("test.conf").is_none());
    }

    #[test]
    fn test_plugin_config_rejects_service_action_without_service() {
        let ttd = TempTedgeDir::new();

        let toml_content = r#"
[[files]]
path = "/etc/test.conf"
type = "test.conf"
service_action = "restart"
"#;
        let config_file = ttd
            .file("action_without_service.toml")
            .with_raw_content(toml_content);

        let config = PluginConfig::new(config_file.utf8_path());

        assert_eq!(config.files.len(), 1);
        assert!(config.get_file_entry(DEFAULT_PLUGIN_CONFIG_TYPE).is_some());
        assert!(config.get_file_entry("test.conf").is_none());
    }

    #[test]
    fn test_plugin_config_defaults_action_to_restart() {
        let ttd = TempTedgeDir::new();

        let toml_content = r#"
[[files]]
path = "/etc/test.conf"
type = "test.conf"
service = "testservice"
"#;
        let config_file = ttd
            .file("service_without_action.toml")
            .with_raw_content(toml_content);

        let config = PluginConfig::new(config_file.utf8_path());

        assert_eq!(config.files.len(), 2);
        let entry = config.get_file_entry("test.conf").unwrap();
        assert_eq!(entry.service, Some("testservice".to_string()));
        assert_eq!(entry.service_action, Some("restart".to_string())); // Defaults to "restart"
    }
}
