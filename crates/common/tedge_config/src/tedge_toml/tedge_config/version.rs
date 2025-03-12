//! Versioning and migrations of the main config file.

use std::borrow::Cow;

use toml::Table;

use super::WritableKey;

#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(into = "&'static str", try_from = "String")]
/// A version of tedge.toml, used to manage migrations (see [Self::migrations])
pub enum TEdgeTomlVersion {
    One,
    Two,
}

impl Default for TEdgeTomlVersion {
    fn default() -> Self {
        Self::One
    }
}

impl TryFrom<String> for TEdgeTomlVersion {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "1" => Ok(Self::One),
            "2" => Ok(Self::Two),
            _ => todo!(),
        }
    }
}

impl From<TEdgeTomlVersion> for &'static str {
    fn from(value: TEdgeTomlVersion) -> Self {
        match value {
            TEdgeTomlVersion::One => "1",
            TEdgeTomlVersion::Two => "2",
        }
    }
}

impl From<TEdgeTomlVersion> for toml::Value {
    fn from(value: TEdgeTomlVersion) -> Self {
        let str: &str = value.into();
        toml::Value::String(str.to_owned())
    }
}

pub enum TomlMigrationStep {
    UpdateFieldValue {
        key: &'static str,
        value: toml::Value,
    },

    MoveKey {
        original: &'static str,
        target: Cow<'static, str>,
    },

    RemoveTableIfEmpty {
        key: &'static str,
    },
}

impl TomlMigrationStep {
    pub fn apply_to(self, mut toml: toml::Value) -> toml::Value {
        match self {
            TomlMigrationStep::MoveKey { original, target } => {
                let mut doc = &mut toml;
                let (tables, field) = original.rsplit_once('.').unwrap();
                for key in tables.split('.') {
                    if doc.as_table().map(|table| table.contains_key(key)) == Some(true) {
                        doc = &mut doc[key];
                    } else {
                        return toml;
                    }
                }
                let value = doc.as_table_mut().unwrap().remove(field);

                if let Some(value) = value {
                    let mut doc = &mut toml;
                    let (tables, field) = target.rsplit_once('.').unwrap();
                    for key in tables.split('.') {
                        let table = doc.as_table_mut().unwrap();
                        if !table.contains_key(key) {
                            table.insert(key.to_owned(), toml::Value::Table(Table::new()));
                        }
                        doc = &mut doc[key];
                    }
                    let table = doc.as_table_mut().unwrap();
                    // TODO if this returns Some, something is going wrong? Maybe this could be an error, or maybe it doesn't matter
                    table.insert(field.to_owned(), value);
                }
            }
            TomlMigrationStep::UpdateFieldValue { key, value } => {
                let mut doc = &mut toml;
                let (tables, field) = key.rsplit_once('.').unwrap();
                for key in tables.split('.') {
                    let table = doc.as_table_mut().unwrap();
                    if !table.contains_key(key) {
                        table.insert(key.to_owned(), toml::Value::Table(Table::new()));
                    }
                    doc = &mut doc[key];
                }
                let table = doc.as_table_mut().unwrap();
                // TODO if this returns Some, something is going wrong? Maybe this could be an error, or maybe it doesn't matter
                table.insert(field.to_owned(), value);
            }
            TomlMigrationStep::RemoveTableIfEmpty { key } => {
                let mut doc = &mut toml;
                let (parents, target) = key.rsplit_once('.').unwrap();
                for key in parents.split('.') {
                    let table = doc.as_table_mut().unwrap();
                    if !table.contains_key(key) {
                        table.insert(key.to_owned(), toml::Value::Table(Table::new()));
                    }
                    doc = &mut doc[key];
                }
                let table = doc.as_table_mut().unwrap();
                if let Some(table) = table.get(target) {
                    let table = table.as_table().unwrap();
                    // TODO make sure this is covered in toml migration test
                    if !table.is_empty() {
                        return toml;
                    }
                }
                table.remove(target);
            }
        }

        toml
    }
}

impl TEdgeTomlVersion {
    fn next(self) -> Self {
        match self {
            Self::One => Self::Two,
            Self::Two => Self::Two,
        }
    }

    /// The migrations to upgrade `tedge.toml` from its current version to the
    /// next version.
    ///
    /// If this returns `None`, the version of `tedge.toml` is the latest
    /// version, and no migrations need to be applied.
    pub fn migrations(self) -> Option<Vec<TomlMigrationStep>> {
        use WritableKey::*;
        let mv = |original, target: WritableKey| TomlMigrationStep::MoveKey {
            original,
            target: target.to_cow_str(),
        };
        let update_version_field = || TomlMigrationStep::UpdateFieldValue {
            key: "config.version",
            value: self.next().into(),
        };
        let rm = |key| TomlMigrationStep::RemoveTableIfEmpty { key };

        match self {
            Self::One => Some(vec![
                mv("mqtt.port", MqttBindPort),
                mv("mqtt.bind_address", MqttBindAddress),
                mv("mqtt.client_host", MqttClientHost),
                mv("mqtt.client_port", MqttClientPort),
                mv("mqtt.client_ca_file", MqttClientAuthCaFile),
                mv("mqtt.client_ca_path", MqttClientAuthCaDir),
                mv("mqtt.client_auth.cert_file", MqttClientAuthCertFile),
                mv("mqtt.client_auth.key_file", MqttClientAuthKeyFile),
                rm("mqtt.client_auth"),
                mv("mqtt.external_port", MqttExternalBindPort),
                mv("mqtt.external_bind_address", MqttExternalBindAddress),
                mv("mqtt.external_bind_interface", MqttExternalBindInterface),
                mv("mqtt.external_capath", MqttExternalCaPath),
                mv("mqtt.external_certfile", MqttExternalCertFile),
                mv("mqtt.external_keyfile", MqttExternalKeyFile),
                mv("az.mapper_timestamp", AzMapperTimestamp(None)),
                mv("aws.mapper_timestamp", AwsMapperTimestamp(None)),
                mv("http.port", HttpBindPort),
                mv("http.bind_address", HttpBindAddress),
                mv("software.default_plugin_type", SoftwarePluginDefault),
                mv("run.lock_files", RunLockFiles),
                mv("firmware.child_update_timeout", FirmwareChildUpdateTimeout),
                mv("c8y.smartrest_templates", C8ySmartrestTemplates(None)),
                update_version_field(),
            ]),
            Self::Two => None,
        }
    }
}

// TODO(marcel): is this being tested somewhere else?
