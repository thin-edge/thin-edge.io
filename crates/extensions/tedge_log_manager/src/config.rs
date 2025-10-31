use camino::Utf8Path;
use camino::Utf8PathBuf;
use serde::Deserialize;
use std::collections::HashMap;
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
#[derive(Clone, Deserialize, Debug, Default, PartialEq, Eq)]
pub struct PluginConfig {
    #[serde(default)]
    pub plugins: HashMap<String, Vec<PluginFilterEntry>>,
}

/// Individual filter entry for a plugin
#[derive(Clone, Deserialize, Debug, PartialEq, Eq)]
pub struct PluginFilterEntry {
    #[serde(default)]
    pub include: Option<String>,
    #[serde(default)]
    pub exclude: Option<String>,
}

impl PluginConfig {
    /// Load plugin configuration from the given path
    pub fn from_file(path: &std::path::Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(contents) => match toml::from_str(&contents) {
                Ok(config) => config,
                Err(err) => {
                    tracing::warn!(
                        "Failed to parse plugin config from {}: {}",
                        path.display(),
                        err
                    );
                    Self::default()
                }
            },
            Err(_) => {
                // File doesn't exist or not readable - this is OK, just use defaults
                Self::default()
            }
        }
    }

    pub(crate) fn get_filters(&self, plugin_name: &str) -> Option<&Vec<PluginFilterEntry>> {
        self.plugins.get(plugin_name)
    }

    /// Apply filtering to log types from a plugin
    ///
    /// Filtering logic:
    /// - If no filters defined, return all types as-is
    /// - When only include is specified: only types matching any include filter are returned
    /// - When only exclude is specified: all types not matching any exclude filter are returned
    /// - When both are specified: include the types that matches any include filter OR doesn't match any exclude filter
    pub fn filter_log_types(&self, plugin_name: &str, log_types: Vec<String>) -> Vec<String> {
        let Some(filters) = self.get_filters(plugin_name) else {
            // No filters defined for the plugin type, include all types
            return log_types;
        };

        let mut exclude_patterns = Vec::new();
        let mut include_patterns = Vec::new();

        for filter in filters {
            if let Some(pattern) = &filter.include {
                if let Ok(compiled) = glob::Pattern::new(pattern) {
                    include_patterns.push(compiled);
                } else {
                    tracing::warn!(
                        "Invalid include pattern for plugin {}: {}",
                        plugin_name,
                        pattern
                    );
                }
            }
            if let Some(pattern) = &filter.exclude {
                if let Ok(compiled) = glob::Pattern::new(pattern) {
                    exclude_patterns.push(compiled);
                } else {
                    tracing::warn!(
                        "Invalid exclude pattern for plugin {}: {}",
                        plugin_name,
                        pattern
                    );
                }
            }
        }

        // If no filter patterns are defined, include all
        if include_patterns.is_empty() && exclude_patterns.is_empty() {
            return log_types;
        }

        // include a type if it matches any include pattern OR doesn't match any exclude pattern
        log_types
            .into_iter()
            .filter(|log_type| {
                let matches_include = include_patterns.iter().any(|p| p.matches(log_type));
                let matches_exclude = exclude_patterns.iter().any(|p| p.matches(log_type));

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
    use crate::config::PluginFilterEntry;
    use std::collections::HashMap;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_plugin_config_from_toml() {
        let toml_content = r#"
[[plugins.journald]]
include = "ssh"

[[plugins.journald]]
include = "tedge-agent"

[[plugins.dmesg]]
exclude = "all"

[[plugins.file]]
exclude = "unwanted-*"
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(toml_content.as_bytes()).unwrap();
        temp_file.flush().unwrap();

        let config = PluginConfig::from_file(temp_file.path());

        // Check journald filters
        let journald_filters = config.get_filters("journald").unwrap();
        assert_eq!(journald_filters.len(), 2);
        assert_eq!(journald_filters[0].include.as_ref().unwrap(), "ssh");
        assert_eq!(journald_filters[1].include.as_ref().unwrap(), "tedge-agent");

        // Check dmesg filters
        let dmesg_filters = config.get_filters("dmesg").unwrap();
        assert_eq!(dmesg_filters.len(), 1);
        assert_eq!(dmesg_filters[0].exclude.as_ref().unwrap(), "all");

        // Check file filters
        let file_filters = config.get_filters("file").unwrap();
        assert_eq!(file_filters.len(), 1);
        assert_eq!(file_filters[0].exclude.as_ref().unwrap(), "unwanted-*");
    }

    #[test]
    fn test_plugin_config_no_filters_returns_all() {
        let config = PluginConfig::default();
        let log_types: Vec<String> = ["ssh", "tedge-agent", "mosquitto"]
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
            vec![
                PluginFilterEntry {
                    include: Some("ssh".to_string()),
                    exclude: None,
                },
                PluginFilterEntry {
                    include: Some("tedge-*".to_string()),
                    exclude: None,
                },
            ],
        );

        let config = PluginConfig { plugins };
        let log_types: Vec<String> = ["ssh", "tedge-agent", "mosquitto", "systemd-logind"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let filtered = config.filter_log_types("journald", log_types);
        assert_eq!(filtered, vec!["ssh".to_string(), "tedge-agent".to_string()]);
    }

    #[test]
    fn test_plugin_filter_exclude_only() {
        let mut plugins = HashMap::new();
        plugins.insert(
            "journald".to_string(),
            vec![
                PluginFilterEntry {
                    include: None,
                    exclude: Some("mosquitto".to_string()),
                },
                PluginFilterEntry {
                    include: None,
                    exclude: Some("systemd-*".to_string()),
                },
            ],
        );

        let config = PluginConfig { plugins };
        let log_types: Vec<String> = [
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
        assert_eq!(filtered, vec!["ssh".to_string(), "tedge-agent".to_string()]);
    }

    #[test]
    fn test_plugin_filter_combined_include_and_exclude() {
        let mut plugins = HashMap::new();
        plugins.insert(
            "journald".to_string(),
            vec![
                PluginFilterEntry {
                    include: None,
                    exclude: Some("systemd-*".to_string()),
                },
                PluginFilterEntry {
                    include: Some("systemd-logind".to_string()),
                    exclude: None,
                },
                PluginFilterEntry {
                    include: Some("tedge-*".to_string()),
                    exclude: None,
                },
            ],
        );

        let config = PluginConfig { plugins };
        let log_types: Vec<String> = [
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
            filtered,
            vec![
                "ssh".to_string(),
                "tedge-agent".to_string(),
                "mosquitto".to_string(),
                "systemd-logind".to_string()
            ]
        );
    }

    #[test]
    fn test_plugin_filter_exclude_all_include_some() {
        let mut plugins = HashMap::new();
        plugins.insert(
            "journald".to_string(),
            vec![
                PluginFilterEntry {
                    include: Some("ssh".to_string()),
                    exclude: None,
                },
                PluginFilterEntry {
                    include: Some("mosquitto".to_string()),
                    exclude: None,
                },
            ],
        );

        let config = PluginConfig { plugins };
        let log_types: Vec<String> = [
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
        assert_eq!(filtered, vec!["ssh".to_string(), "mosquitto".to_string(),]);
    }
}
