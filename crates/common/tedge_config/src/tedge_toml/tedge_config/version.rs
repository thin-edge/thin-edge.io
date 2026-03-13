// Versioning and migrations of the main config file.

use std::borrow::Cow;

use toml::Table;

use super::WritableKey;

#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(into = "&'static str", try_from = "String")]
/// A version of tedge.toml, used to manage migrations (see [Self::migrations])
pub enum TEdgeTomlVersion {
    #[default]
    One,
    Two,
}

impl TryFrom<String> for TEdgeTomlVersion {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "1" => Ok(Self::One),
            "2" => Ok(Self::Two),
            _ => Err(format!("Unknown tedge.toml version: {value}")),
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
    /// Applies the migration step, returning the updated toml and whether
    /// anything actually changed.
    pub fn apply_to(self, mut toml: toml::Value) -> Result<(toml::Value, bool), String> {
        match self {
            TomlMigrationStep::MoveKey { original, target } => {
                let mut doc = &mut toml;
                let (tables, field) = split_key(original);
                for key in tables {
                    if doc.as_table().map(|table| table.contains_key(key)) == Some(true) {
                        doc = &mut doc[key];
                    } else {
                        return Ok((toml, false));
                    }
                }
                let value = doc.as_table_mut().unwrap().remove(field);

                if let Some(value) = value {
                    let mut doc = &mut toml;
                    let (target_tables, target_field) = split_key(&target);
                    for key in target_tables {
                        let table = doc.as_table_mut().unwrap();
                        if !table.contains_key(key) {
                            table.insert(key.to_owned(), toml::Value::Table(Table::new()));
                        }
                        doc = &mut doc[key];
                    }
                    let table = doc.as_table_mut().unwrap();
                    if table.contains_key(target_field) {
                        return Err(format!(
                            "Cannot migrate '{original}' to '{target}': target key already \
                             exists. Please manually remove one of them from tedge.toml."
                        ));
                    }
                    table.insert(target_field.to_owned(), value);
                    Ok((toml, true))
                } else {
                    Ok((toml, false))
                }
            }
            TomlMigrationStep::UpdateFieldValue { key, value } => {
                let mut doc = &mut toml;
                let (tables, field) = split_key(key);
                for key in tables {
                    let table = doc.as_table_mut().unwrap();
                    if !table.contains_key(key) {
                        table.insert(key.to_owned(), toml::Value::Table(Table::new()));
                    }
                    doc = &mut doc[key];
                }
                let table = doc.as_table_mut().unwrap();
                table.insert(field.to_owned(), value);
                Ok((toml, true))
            }
            TomlMigrationStep::RemoveTableIfEmpty { key } => {
                let mut doc = &mut toml;
                let (parents, target) = split_key(key);
                for key in parents {
                    if doc.as_table().map(|t| t.contains_key(key)) != Some(true) {
                        return Ok((toml, false));
                    }
                    doc = &mut doc[key];
                }
                let table = doc.as_table_mut().unwrap();
                if let Some(table) = table.get(target) {
                    let table = table.as_table().unwrap();
                    if !table.is_empty() {
                        return Ok((toml, false));
                    }
                }
                let removed = table.remove(target).is_some();
                Ok((toml, removed))
            }
        }
    }
}

/// Splits a dotted key into (parent table path, final field name).
/// `"a.b.c"` → `(["a", "b"], "c")`
/// `"a"`     → `([], "a")` — top-level key, no intermediate tables
fn split_key(key: &str) -> (Vec<&str>, &str) {
    match key.rsplit_once('.') {
        Some((tables, field)) => (tables.split('.').collect(), field),
        None => (vec![], key),
    }
}

impl TEdgeTomlVersion {
    fn next(self) -> Option<Self> {
        match self {
            Self::One => Some(Self::Two),
            Self::Two => None,
        }
    }

    // NB: when adding a new version V, the *previous* version's steps must include
    // an UpdateFieldValue that bumps config.version to V. Without this, configs
    // already at V-1 will leave the version field unchanged after migration and
    // re-run V-1 steps on every startup.
    fn steps(self) -> Vec<TomlMigrationStep> {
        use WritableKey::*;
        let mv = |original, target: WritableKey| TomlMigrationStep::MoveKey {
            original,
            target: target.to_cow_str(),
        };
        let rm = |key| TomlMigrationStep::RemoveTableIfEmpty { key };

        match self {
            Self::One => vec![
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
                TomlMigrationStep::UpdateFieldValue {
                    key: "config.version",
                    value: Self::Two.into(),
                },
            ],
            Self::Two => vec![TomlMigrationStep::MoveKey {
                original: "apt.dpk",
                target: "apt.dpkg".into(),
            }],
        }
    }

    /// Returns an iterator over all migration steps needed to bring this
    /// version of `tedge.toml` up to date. Yields nothing if already current.
    pub fn migrations(self) -> impl Iterator<Item = TomlMigrationStep> {
        std::iter::successors(Some(self), |v| v.next()).flat_map(|v| v.steps())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_version_string_returns_an_error() {
        assert!(TEdgeTomlVersion::try_from("3".to_string()).is_err());
        assert!(TEdgeTomlVersion::try_from("unknown".to_string()).is_err());
    }

    #[test]
    fn move_key_relocates_an_existing_value() {
        let input = toml::toml!(
            [apt.dpk.options]
            config = "keepold"
        );
        let (output, key_was_moved) = TomlMigrationStep::MoveKey {
            original: "apt.dpk",
            target: Cow::Borrowed("apt.dpkg"),
        }
        .apply_to(toml::Value::Table(input))
        .unwrap();
        assert!(key_was_moved);
        assert_eq!(
            output["apt"]["dpkg"]["options"]["config"].as_str(),
            Some("keepold")
        );
        assert!(output["apt"].as_table().unwrap().get("dpk").is_none());
    }

    #[test]
    fn move_key_is_a_noop_when_source_key_is_absent() {
        let input = toml::toml!(
            [apt.name]
            filter = "tedge.*"
        );
        let expected = input.clone();
        let (output, key_was_moved) = TomlMigrationStep::MoveKey {
            original: "apt.dpk",
            target: Cow::Borrowed("apt.dpkg"),
        }
        .apply_to(toml::Value::Table(input))
        .unwrap();
        assert!(!key_was_moved);
        assert_eq!(output, toml::Value::Table(expected));
    }

    #[test]
    fn move_key_can_move_a_top_level_key() {
        let input = toml::toml!(
            [apt.options]
            config = "keepold"
        );
        let (output, key_was_moved) = TomlMigrationStep::MoveKey {
            original: "apt",
            target: Cow::Borrowed("dpkg"),
        }
        .apply_to(toml::Value::Table(input))
        .unwrap();
        assert!(key_was_moved);
        assert_eq!(
            output["dpkg"]["options"]["config"].as_str(),
            Some("keepold")
        );
        assert!(output.as_table().unwrap().get("apt").is_none());
    }

    #[test]
    fn move_key_errors_when_target_already_exists() {
        let input = toml::toml!(
            [apt.dpk.options]
            config = "from_dpk"

            [apt.dpkg.options]
            config = "from_dpkg"
        );
        let result = TomlMigrationStep::MoveKey {
            original: "apt.dpk",
            target: Cow::Borrowed("apt.dpkg"),
        }
        .apply_to(toml::Value::Table(input));
        assert!(result.is_err());
    }

    #[test]
    fn remove_table_if_empty_removes_an_empty_table() {
        let input = toml::toml!([mqtt.client_auth]);
        let (output, table_was_removed) = TomlMigrationStep::RemoveTableIfEmpty {
            key: "mqtt.client_auth",
        }
        .apply_to(toml::Value::Table(input))
        .unwrap();
        assert!(table_was_removed);
        assert!(output["mqtt"]
            .as_table()
            .unwrap()
            .get("client_auth")
            .is_none());
    }

    #[test]
    fn remove_table_if_empty_preserves_a_non_empty_table() {
        let input = toml::toml!(
            [mqtt.client_auth]
            cert_file = "/path/to/cert.pem"
        );
        let expected = input.clone();
        let (output, table_was_removed) = TomlMigrationStep::RemoveTableIfEmpty {
            key: "mqtt.client_auth",
        }
        .apply_to(toml::Value::Table(input))
        .unwrap();
        assert!(!table_was_removed);
        assert_eq!(output, toml::Value::Table(expected));
    }

    #[test]
    fn remove_table_if_empty_is_a_noop_when_parent_is_absent() {
        let input = toml::toml!(
            [apt]
            name = "tedge"
        );
        let expected = input.clone();
        let (output, table_was_removed) = TomlMigrationStep::RemoveTableIfEmpty {
            key: "mqtt.client_auth",
        }
        .apply_to(toml::Value::Table(input))
        .unwrap();
        assert!(!table_was_removed);
        assert_eq!(output, toml::Value::Table(expected));
    }

    #[test]
    fn v2_migration_renames_apt_dpk_to_apt_dpkg() {
        let input = toml::toml!(
            [config]
            version = "2"

            [apt.dpk.options]
            config = "keepold"
        );
        let (migrated, any_keys_migrated) = TEdgeTomlVersion::Two
            .migrations()
            .try_fold(
                (toml::Value::Table(input), false),
                |(toml, any_migrated), step| {
                    let (toml, step_migrated) = step.apply_to(toml)?;
                    Ok::<_, String>((toml, any_migrated || step_migrated))
                },
            )
            .unwrap();
        assert!(any_keys_migrated);
        assert_eq!(migrated["config"]["version"].as_str(), Some("2"));
        assert_eq!(
            migrated["apt"]["dpkg"]["options"]["config"].as_str(),
            Some("keepold")
        );
        assert!(migrated["apt"].as_table().unwrap().get("dpk").is_none());
    }
}
