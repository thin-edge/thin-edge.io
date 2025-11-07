use camino::Utf8Path;
use camino::Utf8PathBuf;
use regex::Regex;
use serde::Deserialize;
use serde::Deserializer;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::ChannelFilter;
use tedge_api::mqtt_topics::EntityFilter;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::OperationType;
use tedge_config::tedge_toml::ReadError;
use tedge_mqtt_ext::Topic;
use tedge_mqtt_ext::TopicFilter;
use tracing::warn;

pub const DEFAULT_PLUGIN_CONFIG_FILE_NAME: &str = "tedge-log-plugin.toml";
pub const DEFAULT_PLUGIN_CONFIG_DIR_NAME: &str = "plugins/";

/// Configuration of the Configuration Manager
#[derive(Clone, Debug)]
pub struct LogManagerConfig {
    pub mqtt_schema: MqttSchema,
    pub config_dir: PathBuf,
    pub tmp_dir: Arc<Utf8Path>,
    pub log_dir: Utf8PathBuf,
    pub plugin_dirs: Vec<Utf8PathBuf>,
    pub plugin_config_dir: PathBuf,
    pub plugin_config_path: PathBuf,
    pub logtype_reload_topic: Topic,
    pub logfile_request_topic: TopicFilter,
    pub log_metadata_sync_topics: TopicFilter,
    pub sudo_enabled: bool,
}

pub struct LogManagerOptions {
    pub config_dir: PathBuf,
    pub tmp_dir: Arc<Utf8Path>,
    pub log_dir: Utf8PathBuf,
    pub mqtt_schema: MqttSchema,
    pub mqtt_device_topic_id: EntityTopicId,
    pub plugin_dirs: Vec<Utf8PathBuf>,
}

impl LogManagerConfig {
    pub fn from_options(cliopts: LogManagerOptions) -> Result<Self, ReadError> {
        let config_dir = cliopts.config_dir;
        let tmp_dir = cliopts.tmp_dir;
        let log_dir = cliopts.log_dir;
        let mqtt_schema = cliopts.mqtt_schema;
        let mqtt_device_topic_id = cliopts.mqtt_device_topic_id;

        let plugin_config_dir = config_dir.join(DEFAULT_PLUGIN_CONFIG_DIR_NAME);
        let plugin_config_path = plugin_config_dir.join(DEFAULT_PLUGIN_CONFIG_FILE_NAME);

        let logtype_reload_topic = mqtt_schema.topic_for(
            &mqtt_device_topic_id,
            &Channel::CommandMetadata {
                operation: OperationType::LogUpload,
            },
        );

        let logfile_request_topic = mqtt_schema.topics(
            EntityFilter::Entity(&mqtt_device_topic_id),
            ChannelFilter::Command(OperationType::LogUpload),
        );

        let mut log_metadata_sync_topics = mqtt_schema.topics(
            EntityFilter::Entity(&mqtt_device_topic_id),
            ChannelFilter::Command(OperationType::SoftwareUpdate),
        );
        log_metadata_sync_topics.add_all(mqtt_schema.topics(
            EntityFilter::Entity(&mqtt_device_topic_id),
            ChannelFilter::Command(OperationType::ConfigUpdate),
        ));

        Ok(Self {
            mqtt_schema,
            config_dir,
            tmp_dir,
            log_dir,
            plugin_dirs: cliopts.plugin_dirs,
            plugin_config_dir,
            plugin_config_path,
            logtype_reload_topic,
            logfile_request_topic,
            log_metadata_sync_topics,
            sudo_enabled: true,
        })
    }
}

/// Plugin filtering configuration parsed from tedge-log-plugin.toml
#[derive(Clone, Deserialize, Debug, Default)]
struct TomlPluginConfig {
    #[serde(default)]
    plugins: HashMap<String, TomlPluginEntry>,
}

/// Configuration for a single plugin
#[derive(Clone, Deserialize, Debug, Default)]
pub struct TomlPluginEntry {
    #[serde(default)]
    pub filters: Vec<TomlPluginFilterEntry>,
}

/// Individual filter entry for a plugin with compiled regex patterns
#[derive(Clone, Deserialize, Debug)]
pub struct TomlPluginFilterEntry {
    #[serde(default, deserialize_with = "deserialize_regex_pattern")]
    pub include: Option<Regex>,
    #[serde(default, deserialize_with = "deserialize_regex_pattern")]
    pub exclude: Option<Regex>,
}

pub fn deserialize_regex_pattern<'de, D>(deserializer: D) -> Result<Option<Regex>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt_string: Option<String> = Option::deserialize(deserializer)?;
    match opt_string {
        Some(s) => {
            let pattern = Regex::new(&s).map_err(|e| {
                serde::de::Error::custom(format!("Invalid regex pattern '{}': {}", s, e))
            })?;
            Ok(Some(pattern))
        }
        None => Ok(None),
    }
}

/// Plugin configuration (compiled runtime representation)
#[derive(Clone, Debug, Default)]
pub struct PluginConfig {
    pub plugins: HashMap<String, PluginEntry>,
}

#[derive(Clone, Debug, Default)]
pub struct PluginEntry {
    pub filters: PluginFilterConfig,
}

#[derive(Clone, Debug, Default)]
pub struct PluginFilterConfig {
    pub include_patterns: Vec<Regex>,
    pub exclude_patterns: Vec<Regex>,
}

impl From<TomlPluginEntry> for PluginEntry {
    fn from(toml_entry: TomlPluginEntry) -> Self {
        let mut include_patterns = Vec::new();
        let mut exclude_patterns = Vec::new();

        for filter in toml_entry.filters {
            if let Some(pattern) = filter.include {
                include_patterns.push(pattern);
            }
            if let Some(pattern) = filter.exclude {
                exclude_patterns.push(pattern);
            }
        }

        PluginEntry {
            filters: PluginFilterConfig {
                include_patterns,
                exclude_patterns,
            },
        }
    }
}

impl From<TomlPluginConfig> for PluginConfig {
    fn from(toml_config: TomlPluginConfig) -> Self {
        PluginConfig {
            plugins: toml_config
                .plugins
                .into_iter()
                .map(|(k, v)| (k, v.into()))
                .collect(),
        }
    }
}

impl PluginConfig {
    /// Load plugin configuration from the given path
    pub async fn from_file(path: &Path) -> Self {
        match tokio::fs::read_to_string(path).await {
            Ok(contents) => match toml::from_str::<TomlPluginConfig>(&contents) {
                Ok(toml_config) => toml_config.into(),
                Err(err) => {
                    warn!(
                        "Plugin filters not applied due to failure parsing the plugin config {}: {}",
                        path.display(),
                        err
                    );
                    Self::default()
                }
            },
            Err(err) => {
                warn!(
                    "Plugin filters not applied due to failure reading the plugin config {}: {}",
                    path.display(),
                    err
                );
                Self::default()
            }
        }
    }

    pub(crate) fn get_filters(&self, plugin_name: &str) -> Option<&PluginFilterConfig> {
        self.plugins.get(plugin_name).map(|entry| &entry.filters)
    }

    /// Apply filtering to log types from a plugin
    ///
    /// Filtering logic:
    /// - If no filters defined, return all types as-is
    /// - When only include is specified: only types matching any include filter are returned
    /// - When only exclude is specified: all types not matching any exclude filter are returned
    /// - When both are specified: include the types that matches any include filter OR doesn't match any exclude filter
    pub fn filter_log_types(
        &self,
        plugin_name: &str,
        log_types: BTreeSet<String>,
    ) -> BTreeSet<String> {
        let Some(filter_config) = self.get_filters(plugin_name) else {
            // No filters defined for the plugin type, include all types
            return log_types;
        };

        let include_patterns = &filter_config.include_patterns;
        let exclude_patterns = &filter_config.exclude_patterns;

        // If no filter patterns are defined, include all
        if include_patterns.is_empty() && exclude_patterns.is_empty() {
            return log_types;
        }

        // include a type if it matches any include pattern OR doesn't match any exclude pattern
        log_types
            .into_iter()
            .filter(|log_type| {
                let matches_include = include_patterns.iter().any(|p| p.is_match(log_type));
                let matches_exclude = exclude_patterns.iter().any(|p| p.is_match(log_type));

                // When both include and exclude patterns exist, use OR logic
                if !include_patterns.is_empty() && !exclude_patterns.is_empty() {
                    return matches_include || !matches_exclude;
                }

                // When only include patterns exist
                if !include_patterns.is_empty() {
                    return matches_include;
                }

                // When only exclude patterns exist
                if !exclude_patterns.is_empty() {
                    return !matches_exclude;
                }

                true
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::config::PluginConfig;
    use crate::config::PluginEntry;
    use crate::config::PluginFilterConfig;
    use regex::Regex;
    use std::collections::BTreeSet;
    use std::collections::HashMap;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_plugin_config_from_toml() {
        let toml_content = r#"
[[plugins.journald.filters]]
include = "ssh"

[[plugins.journald.filters]]
include = "tedge-agent"

[[plugins.dmesg.filters]]
exclude = "all"

[[plugins.file.filters]]
exclude = "unwanted-.*"
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(toml_content.as_bytes()).unwrap();
        temp_file.flush().unwrap();

        let config = PluginConfig::from_file(temp_file.path()).await;

        // Check journald filters
        let journald_filters = config.get_filters("journald").unwrap();
        assert_eq!(journald_filters.include_patterns.len(), 2);
        assert_eq!(journald_filters.include_patterns[0].as_str(), "ssh");
        assert_eq!(journald_filters.include_patterns[1].as_str(), "tedge-agent");
        assert_eq!(journald_filters.exclude_patterns.len(), 0);

        // Check dmesg filters
        let dmesg_filters = config.get_filters("dmesg").unwrap();
        assert_eq!(dmesg_filters.exclude_patterns.len(), 1);
        assert_eq!(dmesg_filters.exclude_patterns[0].as_str(), "all");
        assert_eq!(dmesg_filters.include_patterns.len(), 0);

        // Check file filters
        let file_filters = config.get_filters("file").unwrap();
        assert_eq!(file_filters.exclude_patterns.len(), 1);
        assert_eq!(file_filters.exclude_patterns[0].as_str(), "unwanted-.*");
        assert_eq!(file_filters.include_patterns.len(), 0);
    }

    #[test]
    fn test_plugin_config_no_filters_returns_all() {
        let config = PluginConfig::default();
        let log_types: BTreeSet<String> = ["ssh", "tedge-agent", "mosquitto"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let filtered = config.filter_log_types("journald", log_types.clone());
        assert_eq!(filtered, log_types);
    }

    #[test]
    fn test_plugin_filter_include_only() {
        let mut plugins = HashMap::new();
        plugins.insert(
            "journald".to_string(),
            PluginEntry {
                filters: PluginFilterConfig {
                    include_patterns: vec![
                        Regex::new("^ssh$").unwrap(),
                        Regex::new("tedge-.*").unwrap(),
                    ],
                    exclude_patterns: vec![],
                },
            },
        );

        let config = PluginConfig { plugins };
        let log_types: BTreeSet<String> = ["ssh", "tedge-agent", "mosquitto", "systemd-logind"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let filtered = config.filter_log_types("journald", log_types);
        assert_eq!(
            filtered.into_iter().collect::<Vec<_>>(),
            vec!["ssh".to_string(), "tedge-agent".to_string()]
        );
    }

    #[test]
    fn test_plugin_filter_exclude_only() {
        let mut plugins = HashMap::new();
        plugins.insert(
            "journald".to_string(),
            PluginEntry {
                filters: PluginFilterConfig {
                    include_patterns: vec![],
                    exclude_patterns: vec![
                        Regex::new("mosquitto").unwrap(),
                        Regex::new("systemd-.*").unwrap(),
                    ],
                },
            },
        );

        let config = PluginConfig { plugins };
        let log_types: BTreeSet<String> = [
            "ssh",
            "tedge-agent",
            "mosquitto",
            "systemd-logind",
            "systemd-journald",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        let filtered = config.filter_log_types("journald", log_types);
        assert_eq!(
            filtered.into_iter().collect::<Vec<_>>(),
            vec!["ssh".to_string(), "tedge-agent".to_string()]
        );
    }

    #[test]
    fn test_plugin_filter_combined_include_and_exclude() {
        let mut plugins = HashMap::new();
        plugins.insert(
            "journald".to_string(),
            PluginEntry {
                filters: PluginFilterConfig {
                    include_patterns: vec![
                        Regex::new("systemd-logind").unwrap(),
                        Regex::new("tedge-.*").unwrap(),
                    ],
                    exclude_patterns: vec![Regex::new("systemd-.*").unwrap()],
                },
            },
        );

        let config = PluginConfig { plugins };
        let log_types: BTreeSet<String> = [
            "ssh",
            "tedge-agent",
            "mosquitto",
            "systemd-logind",
            "systemd-journald",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        let filtered = config.filter_log_types("journald", log_types);
        assert_eq!(
            filtered.into_iter().collect::<Vec<_>>(),
            vec![
                "mosquitto".to_string(),
                "ssh".to_string(),
                "systemd-logind".to_string(),
                "tedge-agent".to_string()
            ]
        );
    }

    #[tokio::test]
    async fn test_invalid_regex_pattern_returns_default_config() {
        let toml_content = r#"
[[plugins.journald.filters]]
include = "[invalid"
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(toml_content.as_bytes()).unwrap();
        temp_file.flush().unwrap();

        let config = PluginConfig::from_file(temp_file.path()).await;

        assert!(config.plugins.is_empty());
    }
}
