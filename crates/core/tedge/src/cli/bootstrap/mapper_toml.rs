//! A custom mapper's `mappers/<name>/mapper.toml`,
//! as bootstrap reads and writes it.
//!
//! Bootstrap only touches a handful of dotted keys
//! (`url`, `device.id`, `auth_method`, `credentials_path`, ...)
//! and leaves the rest of the file — package-shipped content — alone.

use anyhow::Context;
use camino::Utf8Path;
use camino::Utf8PathBuf;

#[derive(Debug, Clone, Default)]
pub struct MapperToml {
    path: Utf8PathBuf,
    table: toml::Table,
}

impl MapperToml {
    /// The config file of the named mapper instance
    pub fn path_for(config_dir: &Utf8Path, name: &str) -> Utf8PathBuf {
        config_dir.join("mappers").join(name).join("mapper.toml")
    }

    /// Load the file; a missing file is an empty table,
    /// a malformed one an error
    pub async fn load(path: &Utf8Path) -> anyhow::Result<Self> {
        let table = match tokio::fs::read_to_string(path).await {
            Ok(content) => content
                .parse()
                .with_context(|| format!("Failed to parse {path}"))?,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => toml::Table::new(),
            Err(err) => return Err(err).with_context(|| format!("Failed to read {path}")),
        };
        Ok(Self {
            path: path.to_owned(),
            table,
        })
    }

    /// Load for a read-only lookup, where a missing or malformed file
    /// simply means "nothing configured"
    pub async fn load_or_empty(path: &Utf8Path) -> Self {
        Self::load(path).await.unwrap_or_else(|_| Self {
            path: path.to_owned(),
            table: toml::Table::new(),
        })
    }

    /// The mapper's directory (where relative paths resolve)
    pub fn mapper_dir(&self) -> &Utf8Path {
        self.path.parent().unwrap_or(&self.path)
    }

    pub fn get(&self, path: &[&str]) -> Option<&toml::Value> {
        toml_nested(Some(&self.table), path)
    }

    /// A non-empty string value at a key path
    pub fn get_str(&self, path: &[&str]) -> Option<&str> {
        self.get(path)?.as_str().filter(|value| !value.is_empty())
    }

    pub fn url(&self) -> Option<&str> {
        self.get_str(&["url"])
    }

    pub fn device_id(&self) -> Option<&str> {
        self.get_str(&["device", "id"])
    }

    pub fn cloud_type(&self) -> Option<&str> {
        self.get_str(&["cloud_type"])
    }

    /// The mapper's credentials file: as configured
    /// (a relative path resolving against the mapper's directory,
    /// as interpreted by the mapper itself),
    /// else `<mapper-dir>/credentials.toml`
    pub fn credentials_path(&self) -> Utf8PathBuf {
        resolve_credentials_path(self.mapper_dir(), self.get_str(&["credentials_path"]))
    }

    /// Set a dotted key, creating intermediate tables as needed
    pub fn set(&mut self, key: &str, value: &str) -> anyhow::Result<()> {
        insert_dotted_key(&mut self.table, key, value)
    }

    /// Remove a dotted key, pruning tables left empty
    pub fn unset(&mut self, key: &str) {
        remove_dotted_key(&mut self.table, key)
    }

    /// Write the file back, creating its directory as needed
    pub async fn save(&self) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("Failed to create directory {parent}"))?;
        }
        tokio::fs::write(&self.path, toml::to_string(&self.table)?)
            .await
            .with_context(|| format!("Failed to write {}", self.path))
    }
}

/// Set dotted keys in a mapper.toml, creating the file as needed
pub async fn write_mapper_config(
    path: &Utf8Path,
    updates: &[(String, String)],
) -> anyhow::Result<()> {
    let mut mapper_toml = MapperToml::load(path).await?;
    for (key, value) in updates {
        mapper_toml.set(key, value)?;
    }
    mapper_toml.save().await
}

/// Resolve a mapper's credentials path:
/// a relative `credentials_path` in `mapper.toml` is relative
/// to the mapper's directory, as interpreted by the mapper itself
pub fn resolve_credentials_path(mapper_dir: &Utf8Path, configured: Option<&str>) -> Utf8PathBuf {
    match configured {
        None => mapper_dir.join("credentials.toml"),
        Some(path) if Utf8Path::new(path).is_relative() => mapper_dir.join(path),
        Some(path) => Utf8PathBuf::from(path),
    }
}

/// Look up a nested value in a TOML table by path, e.g. ["proxy", "bind", "port"]
pub fn toml_nested<'a>(table: Option<&'a toml::Table>, path: &[&str]) -> Option<&'a toml::Value> {
    let mut current = table?;
    let (last, parents) = path.split_last()?;
    for segment in parents {
        current = current.get(*segment)?.as_table()?;
    }
    current.get(*last)
}

/// Insert a value at a dotted key path, e.g. `transport.port`.
///
/// Values that parse as integers or booleans are stored as such,
/// anything else as a string.
fn insert_dotted_key(table: &mut toml::Table, key: &str, value: &str) -> anyhow::Result<()> {
    let mut current = table;
    let mut segments = key.split('.').peekable();
    while let Some(segment) = segments.next() {
        if segments.peek().is_none() {
            current.insert(segment.to_owned(), toml_value(value));
            break;
        }
        let entry = current
            .entry(segment.to_owned())
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));
        current = entry
            .as_table_mut()
            .with_context(|| format!("Config key {key:?} conflicts with an existing value"))?;
    }
    Ok(())
}

fn toml_value(value: &str) -> toml::Value {
    if let Ok(int) = value.parse::<i64>() {
        return toml::Value::Integer(int);
    }
    if let Ok(boolean) = value.parse::<bool>() {
        return toml::Value::Boolean(boolean);
    }
    toml::Value::String(value.to_owned())
}

/// Remove a value at a dotted key path, pruning tables left empty
fn remove_dotted_key(table: &mut toml::Table, key: &str) {
    match key.split_once('.') {
        None => {
            table.remove(key);
        }
        Some((head, rest)) => {
            if let Some(child) = table.get_mut(head).and_then(|value| value.as_table_mut()) {
                remove_dotted_key(child, rest);
                if child.is_empty() {
                    table.remove(head);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_credentials_path_is_resolved_against_the_mapper_dir() {
        let mapper_dir = Utf8Path::new("/etc/tedge/mappers/thingsboard");
        assert_eq!(
            resolve_credentials_path(mapper_dir, Some("credentials.toml")),
            "/etc/tedge/mappers/thingsboard/credentials.toml"
        );
        assert_eq!(
            resolve_credentials_path(mapper_dir, None),
            "/etc/tedge/mappers/thingsboard/credentials.toml"
        );
        assert_eq!(
            resolve_credentials_path(mapper_dir, Some("/run/secrets/tb.toml")),
            "/run/secrets/tb.toml"
        );
    }

    #[test]
    fn dotted_keys_create_nested_tables_with_typed_values() {
        let mut table = toml::Table::new();
        insert_dotted_key(&mut table, "url", "tb.example.com").unwrap();
        insert_dotted_key(&mut table, "transport.port", "8883").unwrap();
        insert_dotted_key(&mut table, "transport.tls", "true").unwrap();
        assert_eq!(table["url"].as_str(), Some("tb.example.com"));
        assert_eq!(table["transport"]["port"].as_integer(), Some(8883));
        assert_eq!(table["transport"]["tls"].as_bool(), Some(true));
    }

    #[test]
    fn dotted_key_conflicting_with_value_is_an_error() {
        let mut table = toml::Table::new();
        insert_dotted_key(&mut table, "url", "tb.example.com").unwrap();
        assert!(insert_dotted_key(&mut table, "url.port", "8883").is_err());
    }

    #[test]
    fn removing_dotted_keys_prunes_empty_tables_and_keeps_the_rest() {
        let mut table: toml::Table = r#"
url = "tb.example.com"
auth_method = "certificate"

[device]
id = "demo01"

[bridge]
topic_prefix = "acme"
custom_rule = "keep-me"

[transport]
port = 8883
"#
        .parse()
        .unwrap();
        remove_dotted_key(&mut table, "url");
        remove_dotted_key(&mut table, "auth_method");
        remove_dotted_key(&mut table, "device.id");
        remove_dotted_key(&mut table, "bridge.topic_prefix");
        remove_dotted_key(&mut table, "credentials_path"); // absent: no-op

        // bootstrap-managed keys are gone, emptied tables pruned
        assert!(!table.contains_key("url"));
        assert!(!table.contains_key("device"));
        // package-shipped content survives
        assert_eq!(table["bridge"]["custom_rule"].as_str(), Some("keep-me"));
        assert_eq!(table["transport"]["port"].as_integer(), Some(8883));
    }

    #[tokio::test]
    async fn mapper_toml_round_trips_through_the_file_system() {
        let tmp = tempfile::tempdir().unwrap();
        let config_dir = Utf8Path::from_path(tmp.path()).unwrap();
        let path = MapperToml::path_for(config_dir, "acme");

        // a missing file is an empty table
        let mut mapper_toml = MapperToml::load(&path).await.unwrap();
        assert_eq!(mapper_toml.url(), None);
        assert_eq!(
            mapper_toml.credentials_path(),
            config_dir.join("mappers/acme/credentials.toml")
        );

        mapper_toml.set("url", "acme.example.com").unwrap();
        mapper_toml.set("device.id", "acme01").unwrap();
        mapper_toml
            .set("credentials_path", "secrets/creds.toml")
            .unwrap();
        mapper_toml.save().await.unwrap();

        let reloaded = MapperToml::load(&path).await.unwrap();
        assert_eq!(reloaded.url(), Some("acme.example.com"));
        assert_eq!(reloaded.device_id(), Some("acme01"));
        assert_eq!(reloaded.cloud_type(), None);
        // relative credentials paths resolve against the mapper directory
        assert_eq!(
            reloaded.credentials_path(),
            config_dir.join("mappers/acme/secrets/creds.toml")
        );

        // empty strings count as unset
        write_mapper_config(&path, &[("url".to_owned(), String::new())])
            .await
            .unwrap();
        assert_eq!(MapperToml::load(&path).await.unwrap().url(), None);

        // a malformed file is an error, not silently empty
        tokio::fs::write(&path, "url = [unclosed").await.unwrap();
        assert!(MapperToml::load(&path).await.is_err());
        assert_eq!(MapperToml::load_or_empty(&path).await.url(), None);
    }
}
