//! Changing a mapper's `cloud_type`, which changes the schema its file is
//! read with

use camino::Utf8Path;
use tedge_config_engine::ops::Action;
use tedge_config_engine::ConfigError;
use tedge_utils::paths::ManagedFile;

use crate::cloud_schema::CloudSchema;
use crate::CloudType;

/// What changing a mapper's `cloud_type` does to its file
pub(crate) enum Change {
    /// The schema stays the same, so the write is an ordinary one
    SameSchema,
    /// The file has been rewritten under the new schema
    Converted,
}

/// Applies `action` to the `cloud_type` of the mapper `name` if that changes
/// the schema of its file
///
/// The file is rewritten under the new schema: keys the new schema doesn't
/// have, or whose value it doesn't accept, are deleted. Unless `force` is
/// set, a change that would delete any key is refused, naming every one.
/// The rewrite works on the file's TOML rather than through either schema,
/// as the two can read a key shared by both as different types.
pub(crate) async fn change_cloud_type(
    config_dir: &Utf8Path,
    name: &str,
    file: &ManagedFile,
    key: &str,
    action: &Action,
    force: bool,
) -> Result<Change, ConfigError> {
    let current = CloudSchema::resolve(name, file).await;
    let Some(mut table) = read_table(file).await else {
        // Leave a file that doesn't load to the ordinary write, which reports why
        return Ok(Change::SameSchema);
    };
    match action {
        Action::Set(value) | Action::Add(value) => {
            if value.parse::<CloudType>().is_err() {
                // The ordinary write rejects it, listing the accepted values
                return Ok(Change::SameSchema);
            }
            table.insert("cloud_type".into(), value.clone().into());
        }
        Action::Unset => {
            table.remove("cloud_type");
        }
        Action::Remove(value) => {
            if table.get("cloud_type").and_then(|v| v.as_str()) == Some(value) {
                table.remove("cloud_type");
            }
        }
    }
    let new = match table.get("cloud_type").and_then(|v| v.as_str()) {
        Some(cloud_type) => {
            CloudSchema::from(cloud_type.parse::<CloudType>().map_err(|reason| {
                ConfigError::InvalidKeyValue {
                    key: key.to_owned(),
                    reason,
                }
            })?)
        }
        None => CloudSchema::from_name(name),
    };
    if new == current {
        return Ok(Change::SameSchema);
    }

    let prefix = key.strip_suffix("cloud_type").unwrap_or_default();
    let dropped = drop_keys_outside(new, config_dir, &mut table);
    if !dropped.is_empty() && !force {
        let dropped: Vec<_> = dropped.iter().map(|k| format!("{prefix}{k}")).collect();
        return Err(ConfigError::InvalidKeyValue {
            key: key.to_owned(),
            reason: format!(
                "the {new} schema has no place for these keys, which the change would delete: {}. \
                 Run the command again with --force to delete them",
                dropped.join(", ")
            ),
        });
    }
    write_table(file, &table).await?;
    Ok(Change::Converted)
}

/// Deletes the keys of `table` that `schema` doesn't have or whose value it
/// doesn't accept, returning them
fn drop_keys_outside(
    schema: CloudSchema,
    config_dir: &Utf8Path,
    table: &mut toml::Table,
) -> Vec<String> {
    let mut dropped = Vec::new();
    for (key, value) in leaves(table, "") {
        if key == "cloud_type" {
            continue;
        }
        let mut single = toml::Table::new();
        insert_at(&mut single, &key, value);
        if !schema.accepts(config_dir, &key, &single) {
            dropped.push(key);
        }
    }
    for key in &dropped {
        remove_at(table, key);
    }
    dropped
}

/// The leaf values of `table` with their dotted keys
fn leaves(table: &toml::Table, prefix: &str) -> Vec<(String, toml::Value)> {
    table
        .iter()
        .flat_map(|(name, value)| {
            let key = format!("{prefix}{name}");
            match value {
                toml::Value::Table(group) => leaves(group, &format!("{key}.")),
                leaf => vec![(key, leaf.clone())],
            }
        })
        .collect()
}

fn insert_at(table: &mut toml::Table, key: &str, value: toml::Value) {
    match key.split_once('.') {
        Some((group, rest)) => {
            let group = table
                .entry(group)
                .or_insert_with(|| toml::Table::new().into());
            if let toml::Value::Table(group) = group {
                insert_at(group, rest, value);
            }
        }
        None => {
            table.insert(key.to_owned(), value);
        }
    }
}

/// Removes `key` from `table`, along with any group it leaves empty
fn remove_at(table: &mut toml::Table, key: &str) {
    match key.split_once('.') {
        Some((group, rest)) => {
            if let Some(toml::Value::Table(inner)) = table.get_mut(group) {
                remove_at(inner, rest);
                if inner.is_empty() {
                    table.remove(group);
                }
            }
        }
        None => {
            table.remove(key);
        }
    }
}

/// The file's table, or `None` if it can't be read or parsed
async fn read_table(file: &ManagedFile) -> Option<toml::Table> {
    match file.read_to_string().await.ok()? {
        Some(content) => content.parse().ok(),
        None => Some(toml::Table::new()),
    }
}

async fn write_table(file: &ManagedFile, table: &toml::Table) -> Result<(), ConfigError> {
    let write_error = |reason: String| ConfigError::WriteFile {
        path: file.path().to_owned(),
        reason,
    };
    let content = toml::to_string(table).map_err(|e| write_error(e.to_string()))?;
    // Like every other config write, a file that can't be given tedge
    // ownership is still written
    let file = file.clone().warn_and_ignore_permission_errors();
    let parent = file.parent();
    parent
        .ensure()
        .await
        .map_err(|e| write_error(format!("creating {parent}: {e}")))?;
    file.replace_atomic(content)
        .await
        .map_err(|e| write_error(e.to_string()))
}
