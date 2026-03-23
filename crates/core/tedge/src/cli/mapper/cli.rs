use crate::command::BuildCommand;
use crate::command::Command;
use crate::log::MaybeFancy;
use crate::ConfigError;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use pad::PadStr;
use tedge_config::TEdgeConfig;
use yansi::Paint;

#[derive(clap::Subcommand, Debug)]
pub enum MapperCli {
    /// List available mappers and their cloud type
    List,

    /// Read a value from a mapper's config
    Config {
        #[clap(subcommand)]
        cmd: MapperConfigCmd,
    },
}

#[derive(clap::Subcommand, Debug)]
pub enum MapperConfigCmd {
    /// Get a config value from a mapper's mapper.toml
    ///
    /// The key is in the form `<mapper-name>.<toml-key-path>`, e.g. `thingsboard.url`
    /// or `thingsboard.device.cert_path`.
    Get {
        /// The key to look up, e.g. `thingsboard.url`
        key: String,
    },
}

#[async_trait::async_trait]
impl BuildCommand for MapperCli {
    async fn build_command(self, config: &TEdgeConfig) -> Result<Box<dyn Command>, ConfigError> {
        let mappers_root = config.root_dir().join("mappers");
        match self {
            MapperCli::List => Ok(ListMappersCommand { mappers_root }.into_boxed()),
            MapperCli::Config {
                cmd: MapperConfigCmd::Get { key },
            } => {
                let (mapper_name, toml_key) = split_mapper_key(&key)?;
                Ok(MapperConfigGetCommand {
                    mappers_root,
                    mapper_name,
                    toml_key,
                }
                .into_boxed())
            }
        }
    }
}

/// Splits `thingsboard.device.cert_path` into `("thingsboard", "device.cert_path")`.
fn split_mapper_key(key: &str) -> Result<(String, String), ConfigError> {
    match key.split_once('.') {
        Some((name, rest)) => Ok((name.to_string(), rest.to_string())),
        None => Err(anyhow::anyhow!(
            "Invalid key '{key}': expected format '<mapper-name>.<toml-key>', e.g. 'thingsboard.url'"
        )
        .into()),
    }
}

/// `tedge mapper list` — prints all mappers under the mappers root with their cloud type.
struct ListMappersCommand {
    mappers_root: Utf8PathBuf,
}

#[async_trait::async_trait]
impl Command for ListMappersCommand {
    fn description(&self) -> String {
        "list available mappers".to_string()
    }

    async fn execute(&self, _config: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        let mappers = scan_mappers(&self.mappers_root).await;
        if mappers.is_empty() {
            eprintln!("No mappers found under '{}'", self.mappers_root);
            return Ok(());
        }
        let max_width = mappers.iter().map(|(name, _)| name.len()).max().unwrap_or(0);
        for (name, cloud_type) in &mappers {
            let padded = name.pad_to_width_with_alignment(max_width, pad::Alignment::Right);
            if let Some(cloud) = cloud_type {
                println!(
                    "{}  {}",
                    padded.yellow(),
                    format!("cloud_type={cloud}").dim()
                );
            } else {
                println!("{}", padded.yellow());
            }
        }
        Ok(())
    }
}

/// `tedge mapper config get thingsboard.url` — reads a TOML key from a mapper's mapper.toml.
struct MapperConfigGetCommand {
    mappers_root: Utf8PathBuf,
    mapper_name: String,
    toml_key: String,
}

#[async_trait::async_trait]
impl Command for MapperConfigGetCommand {
    fn description(&self) -> String {
        format!(
            "get config key '{}' for mapper '{}'",
            self.toml_key, self.mapper_name
        )
    }

    async fn execute(&self, _config: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        let mapper_dir = self.mappers_root.join(&self.mapper_name);
        if !tokio::fs::try_exists(&mapper_dir).await.unwrap_or(false) {
            let available = scan_mappers(&self.mappers_root).await;
            let err: anyhow::Error = if available.is_empty() {
                anyhow::anyhow!(
                    "Mapper '{}' not found. No mappers configured under '{}'.",
                    self.mapper_name,
                    self.mappers_root
                )
            } else {
                let names: Vec<_> = available.into_iter().map(|(n, _)| n).collect();
                anyhow::anyhow!(
                    "Mapper '{}' not found. Available mappers: {}",
                    self.mapper_name,
                    names.join(", ")
                )
            };
            return Err(err.into());
        }

        let mapper_toml_path = mapper_dir.join("mapper.toml");
        let content = tokio::fs::read_to_string(&mapper_toml_path)
            .await
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    anyhow::anyhow!(
                        "Mapper '{}' has no mapper.toml at '{}'",
                        self.mapper_name,
                        mapper_toml_path
                    )
                } else {
                    anyhow::anyhow!("Failed to read '{}': {e}", mapper_toml_path)
                }
            })?;

        let table: toml::Table = content
            .parse()
            .map_err(|e| anyhow::anyhow!("Failed to parse '{}': {e}", mapper_toml_path))?;

        let value = walk_toml_key(&table, &self.toml_key).ok_or_else(|| {
            anyhow::anyhow!(
                "Key '{}' not found in '{}'",
                self.toml_key,
                mapper_toml_path
            )
        })?;

        println!("{}", toml_value_to_string(value));
        Ok(())
    }
}

/// Walks a dotted key path through a TOML table, e.g. `"device.cert_path"`.
fn walk_toml_key<'a>(table: &'a toml::Table, key: &str) -> Option<&'a toml::Value> {
    let mut parts = key.splitn(2, '.');
    let head = parts.next()?;
    let value = table.get(head)?;
    match parts.next() {
        None => Some(value),
        Some(rest) => match value {
            toml::Value::Table(inner) => walk_toml_key(inner, rest),
            _ => None, // intermediate node is not a table
        },
    }
}

/// Renders a TOML value as a plain string for display (matching `tedge config get` style).
fn toml_value_to_string(value: &toml::Value) -> String {
    match value {
        toml::Value::String(s) => s.clone(),
        toml::Value::Integer(i) => i.to_string(),
        toml::Value::Float(f) => f.to_string(),
        toml::Value::Boolean(b) => b.to_string(),
        toml::Value::Datetime(d) => d.to_string(),
        toml::Value::Array(_) | toml::Value::Table(_) => value.to_string(),
    }
}

/// Scans `mappers_root` and returns `(name, cloud_type)` for each subdirectory.
///
/// Every subdirectory is treated as a potential mapper — a flows-only mapper
/// may have no `mapper.toml` and no `flows/` directory before its first
/// startup (the `flows/` directory is created automatically when the mapper
/// starts). If a `mapper.toml` is present, its `cloud_type` field is read and
/// included in the output.
async fn scan_mappers(mappers_root: &Utf8Path) -> Vec<(String, Option<String>)> {
    let Ok(mut entries) = tokio::fs::read_dir(mappers_root).await else {
        return Vec::new();
    };

    let mut mappers = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        let Ok(ft) = entry.file_type().await else {
            continue;
        };
        if !ft.is_dir() {
            continue;
        }
        let path = Utf8PathBuf::from(entry.path().to_string_lossy().into_owned());
        let mapper_toml = path.join("mapper.toml");
        let has_mapper_toml = tokio::fs::try_exists(&mapper_toml).await.unwrap_or(false);
        let name = entry.file_name().to_string_lossy().into_owned();
        let cloud_type = if has_mapper_toml {
            read_cloud_type(&mapper_toml).await
        } else {
            None
        };
        mappers.push((name, cloud_type));
    }
    mappers.sort_by(|(a, _), (b, _)| a.cmp(b));
    mappers
}

/// Reads the `cloud_type` string from a `mapper.toml` if present.
async fn read_cloud_type(mapper_toml: &Utf8Path) -> Option<String> {
    let content = tokio::fs::read_to_string(mapper_toml).await.ok()?;
    let table: toml::Table = content.parse().ok()?;
    table
        .get("cloud_type")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tedge_test_utils::fs::TempTedgeDir;

    mod list_mappers {
        use super::*;

        #[tokio::test]
        async fn empty_mappers_dir_returns_empty() {
            let ttd = TempTedgeDir::new();
            let mappers_root = ttd.utf8_path().join("mappers");
            tokio::fs::create_dir_all(&mappers_root).await.unwrap();

            assert!(scan_mappers(&mappers_root).await.is_empty());
        }

        #[tokio::test]
        async fn dir_without_mapper_toml_or_flows_is_included() {
            // A mapper directory before first startup has no flows/ yet — it
            // should still appear in the list.
            let ttd = TempTedgeDir::new();
            let mappers_root = ttd.utf8_path().join("mappers");
            tokio::fs::create_dir_all(mappers_root.join("mymapper"))
                .await
                .unwrap();

            let mappers = scan_mappers(&mappers_root).await;
            assert_eq!(mappers, vec![("mymapper".to_string(), None)]);
        }

        #[tokio::test]
        async fn flows_only_mapper_is_included() {
            let ttd = TempTedgeDir::new();
            let mappers_root = ttd.utf8_path().join("mappers");
            let flows_dir = mappers_root.join("thingsboard/flows");
            tokio::fs::create_dir_all(&flows_dir).await.unwrap();
            tokio::fs::write(
                flows_dir.join("telemetry.toml"),
                "input.mqtt.topics = [\"te/+/+/+/+/m/+\"]\n",
            )
            .await
            .unwrap();

            let mappers = scan_mappers(&mappers_root).await;
            assert_eq!(mappers, vec![("thingsboard".to_string(), None)]);
        }

        #[tokio::test]
        async fn flows_only_mapper_without_flows_dir_is_included() {
            // Before first startup the flows/ dir doesn't exist yet — mapper should still appear.
            let ttd = TempTedgeDir::new();
            let mappers_root = ttd.utf8_path().join("mappers");
            tokio::fs::create_dir_all(mappers_root.join("thingsboard"))
                .await
                .unwrap();

            let mappers = scan_mappers(&mappers_root).await;
            assert_eq!(mappers, vec![("thingsboard".to_string(), None)]);
        }

        #[tokio::test]
        async fn flows_only_mapper_with_empty_flows_dir_is_included() {
            let ttd = TempTedgeDir::new();
            let mappers_root = ttd.utf8_path().join("mappers");
            tokio::fs::create_dir_all(mappers_root.join("thingsboard/flows"))
                .await
                .unwrap();

            let mappers = scan_mappers(&mappers_root).await;
            assert_eq!(mappers, vec![("thingsboard".to_string(), None)]);
        }

        #[tokio::test]
        async fn mapper_with_both_mapper_toml_and_flows_is_included_once() {
            let ttd = TempTedgeDir::new();
            let mappers_root = ttd.utf8_path().join("mappers");
            let mapper_dir = mappers_root.join("c8y");
            tokio::fs::create_dir_all(mapper_dir.join("flows"))
                .await
                .unwrap();
            tokio::fs::write(mapper_dir.join("mapper.toml"), "cloud_type = \"c8y\"\n")
                .await
                .unwrap();

            let mappers = scan_mappers(&mappers_root).await;
            assert_eq!(mappers, vec![("c8y".to_string(), Some("c8y".to_string()))]);
        }

        #[tokio::test]
        async fn lists_mapper_with_cloud_type() {
            let ttd = TempTedgeDir::new();
            let mappers_root = ttd.utf8_path().join("mappers");
            let c8y_dir = mappers_root.join("c8y");
            tokio::fs::create_dir_all(&c8y_dir).await.unwrap();
            tokio::fs::write(c8y_dir.join("mapper.toml"), "cloud_type = \"c8y\"\n")
                .await
                .unwrap();

            let mappers = scan_mappers(&mappers_root).await;
            assert_eq!(mappers, vec![("c8y".to_string(), Some("c8y".to_string()))]);
        }

        #[tokio::test]
        async fn lists_mapper_without_cloud_type() {
            let ttd = TempTedgeDir::new();
            let mappers_root = ttd.utf8_path().join("mappers");
            let tb_dir = mappers_root.join("thingsboard");
            tokio::fs::create_dir_all(&tb_dir).await.unwrap();
            tokio::fs::write(
                tb_dir.join("mapper.toml"),
                "url = \"tb.example.com:8883\"\n",
            )
            .await
            .unwrap();

            let mappers = scan_mappers(&mappers_root).await;
            assert_eq!(mappers, vec![("thingsboard".to_string(), None)]);
        }

        #[tokio::test]
        async fn lists_mixed_built_in_and_user_defined() {
            let ttd = TempTedgeDir::new();
            let mappers_root = ttd.utf8_path().join("mappers");

            for (name, content) in [
                ("c8y", "cloud_type = \"c8y\"\n"),
                ("thingsboard", "url = \"tb.example.com:8883\"\n"),
            ] {
                let dir = mappers_root.join(name);
                tokio::fs::create_dir_all(&dir).await.unwrap();
                tokio::fs::write(dir.join("mapper.toml"), content)
                    .await
                    .unwrap();
            }
            // directory without mapper.toml — now included as a flows-only mapper
            tokio::fs::create_dir_all(mappers_root.join("myflows"))
                .await
                .unwrap();

            let mappers = scan_mappers(&mappers_root).await;
            assert_eq!(mappers.len(), 3);
            assert!(mappers
                .iter()
                .any(|(n, ct)| n == "c8y" && ct.as_deref() == Some("c8y")));
            assert!(mappers
                .iter()
                .any(|(n, ct)| n == "thingsboard" && ct.is_none()));
        }
    }

    mod config_get {
        use super::*;

        #[test]
        fn splits_key_correctly() {
            let (name, key) = split_mapper_key("thingsboard.url").unwrap();
            assert_eq!(name, "thingsboard");
            assert_eq!(key, "url");
        }

        #[test]
        fn splits_nested_key_correctly() {
            let (name, key) = split_mapper_key("thingsboard.device.cert_path").unwrap();
            assert_eq!(name, "thingsboard");
            assert_eq!(key, "device.cert_path");
        }

        #[test]
        fn errors_without_dot() {
            assert!(split_mapper_key("thingsboard").is_err());
        }

        #[test]
        fn walk_top_level_key() {
            let table: toml::Table = "url = \"mqtt.example.com\"\n".parse().unwrap();
            let val = walk_toml_key(&table, "url").unwrap();
            assert_eq!(toml_value_to_string(val), "mqtt.example.com");
        }

        #[test]
        fn walk_nested_key() {
            let table: toml::Table =
                "[device]\ncert_path = \"/etc/tedge/device-certs/tedge-certificate.pem\"\n"
                    .parse()
                    .unwrap();
            let val = walk_toml_key(&table, "device.cert_path").unwrap();
            assert_eq!(
                toml_value_to_string(val),
                "/etc/tedge/device-certs/tedge-certificate.pem"
            );
        }

        #[test]
        fn missing_key_returns_none() {
            let table: toml::Table = "url = \"mqtt.example.com\"\n".parse().unwrap();
            assert!(walk_toml_key(&table, "device.cert_path").is_none());
        }

        #[test]
        fn non_table_intermediate_returns_none() {
            let table: toml::Table = "url = \"mqtt.example.com\"\n".parse().unwrap();
            // "url" is a string, not a table — can't descend further
            assert!(walk_toml_key(&table, "url.host").is_none());
        }
    }
}
