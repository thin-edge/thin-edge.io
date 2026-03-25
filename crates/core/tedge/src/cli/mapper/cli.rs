use crate::command::BuildCommand;
use crate::command::Command;
use crate::log::MaybeFancy;
use crate::ConfigError;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use pad::PadStr;
use tedge_config::TEdgeConfig;
use tedge_mapper::custom_mapper_config::load_mapper_config;
use tedge_mapper::custom_mapper_config::scan_mappers_shallow;
use tedge_mapper::custom_mapper_resolve::resolve_effective_config;
use tedge_mapper::custom_mapper_resolve::ConfigSource;
use tedge_mapper::custom_mapper_resolve::EffectiveMapperConfig;
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
    ///
    /// Schema-level keys (`url`, `device.id`, `device.cert_path`, `device.key_path`,
    /// `device.root_cert_path`) return the *effective* value — including cert CN inference
    /// and `tedge.toml` fallbacks — with a source annotation on stderr.
    /// All other keys are read directly from `mapper.toml`.
    Get {
        /// The key to look up, e.g. `thingsboard.url`
        key: String,
    },
}

/// Schema-level keys handled by `resolve_effective_config` rather than raw TOML walk.
const SCHEMA_KEYS: &[&str] = &[
    "url",
    "device.id",
    "device.cert_path",
    "device.key_path",
    "device.root_cert_path",
];

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

/// One row of output for `tedge mapper list`.
struct MapperRow {
    name: String,
    cloud_type: String,
    url: String,
    device_id: String,
}

/// Builds the display rows for `tedge mapper list` by resolving the effective
/// configuration for each mapper. Errors for individual mappers are swallowed
/// so that one broken mapper does not prevent the rest from being listed.
async fn build_mapper_rows(
    mappers_root: &Utf8Path,
    mappers: &[(String, Option<toml::Table>)],
    config: &TEdgeConfig,
) -> Vec<MapperRow> {
    let mut rows = Vec::with_capacity(mappers.len());
    for (name, raw_table) in mappers {
        let cloud_type = raw_table
            .as_ref()
            .and_then(|t| t.get("cloud_type"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let mapper_dir = mappers_root.join(name);
        let (url, device_id) = match load_mapper_config(&mapper_dir).await {
            Ok(Some(raw)) => match resolve_effective_config(&raw, config).await {
                Ok(effective) => {
                    let url = effective
                        .url
                        .map(|u| u.value.to_string())
                        .unwrap_or_default();
                    let device_id = effective
                        .device_id
                        .map(|d| format!("{} [{}]", d.value, d.source.short_tag()))
                        .unwrap_or_default();
                    (url, device_id)
                }
                Err(_) => (String::new(), String::new()),
            },
            _ => (String::new(), String::new()),
        };
        rows.push(MapperRow {
            name: name.clone(),
            cloud_type,
            url,
            device_id,
        });
    }
    rows
}

/// `tedge mapper list` — prints all mappers under the mappers root with their
/// cloud type, url, and effective device identity.
struct ListMappersCommand {
    mappers_root: Utf8PathBuf,
}

#[async_trait::async_trait]
impl Command for ListMappersCommand {
    fn description(&self) -> String {
        "list available mappers".to_string()
    }

    async fn execute(&self, config: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        let mappers = scan_mappers_shallow(&self.mappers_root).await;
        if mappers.is_empty() {
            eprintln!("No mappers found under '{}'", self.mappers_root);
            return Ok(());
        }

        let rows = build_mapper_rows(&self.mappers_root, &mappers, &config).await;

        let name_w = rows.iter().map(|r| r.name.len()).max().unwrap_or(0);
        let cloud_type_w = rows.iter().map(|r| r.cloud_type.len()).max().unwrap_or(0);
        let url_w = rows
            .iter()
            .map(|r| r.url.len())
            .max()
            .unwrap_or(0)
            .max("url".len());

        for row in &rows {
            let name = row
                .name
                .pad_to_width_with_alignment(name_w, pad::Alignment::Right);
            let cloud_type = row.cloud_type.pad_to_width(cloud_type_w);
            let url = row.url.pad_to_width(url_w);
            if row.device_id.is_empty() && row.url.is_empty() && row.cloud_type.is_empty() {
                println!("{}", name.yellow());
            } else {
                println!(
                    "{}  {}  {}  {}",
                    name.yellow(),
                    url.dim(),
                    row.device_id.dim(),
                    cloud_type.dim(),
                );
            }
        }
        Ok(())
    }
}

/// `tedge mapper config get thingsboard.url` — returns an effective config value.
///
/// Schema-level keys (`url`, `device.id`, `device.cert_path`, `device.key_path`,
/// `device.root_cert_path`) are resolved via [`resolve_effective_config`] so that
/// cert CN inference and `tedge.toml` fallbacks are applied. A source annotation
/// (e.g. `# inferred from certificate CN (...)`) is written to stderr.
///
/// All other keys are read directly from `mapper.toml` with a `# from mapper.toml`
/// annotation on stderr.
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

    async fn execute(&self, config: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        let mapper_dir = self.mappers_root.join(&self.mapper_name);
        if !tokio::fs::try_exists(&mapper_dir).await.unwrap_or(false) {
            let available = scan_mappers_shallow(&self.mappers_root).await;
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

        let raw = load_mapper_config(&mapper_dir).await?.ok_or_else(|| {
            anyhow::anyhow!(
                "Mapper '{}' has no mapper.toml at '{mapper_dir}'",
                self.mapper_name
            )
        })?;

        if SCHEMA_KEYS.contains(&self.toml_key.as_str()) {
            let effective = resolve_effective_config(&raw, &config).await?;
            print_schema_key(&self.toml_key, &effective, &mapper_dir)?;
        } else {
            let value = walk_toml_key(&raw.table, &self.toml_key).ok_or_else(|| {
                anyhow::anyhow!(
                    "Key '{}' not found in '{mapper_dir}/mapper.toml'",
                    self.toml_key
                )
            })?;
            println!("{}", toml_value_to_string(value));
            eprintln!("# {}", ConfigSource::MapperToml);
        }

        Ok(())
    }
}

/// Prints the effective value of a schema-level key to stdout and its source annotation
/// to stderr.  Returns an error if the key has no value (e.g. `device.id` when the cert
/// is unreadable under cert auth).
fn print_schema_key(
    key: &str,
    effective: &EffectiveMapperConfig,
    mapper_dir: &camino::Utf8Path,
) -> anyhow::Result<()> {
    match key {
        "url" => {
            let s = effective.url.as_ref().ok_or_else(|| {
                anyhow::anyhow!("Key 'url' not set in '{mapper_dir}/mapper.toml'")
            })?;
            println!("{}", s.value);
            eprintln!("# {}", s.source);
        }
        "device.id" => {
            let s = effective.device_id.as_ref().ok_or_else(|| {
                anyhow::anyhow!(
                    "Cannot determine device.id for mapper at '{mapper_dir}': \
                     certificate authentication is configured but the certificate is unreadable. \
                     Set 'device.id' explicitly in '{mapper_dir}/mapper.toml' to override."
                )
            })?;
            println!("{}", s.value);
            eprintln!("# {}", s.source);
        }
        "device.cert_path" => {
            let s = effective.cert_path.as_ref().ok_or_else(|| {
                anyhow::anyhow!(
                    "Key 'device.cert_path' is not set in '{mapper_dir}/mapper.toml' \
                     and is not configured in tedge.toml"
                )
            })?;
            println!("{}", s.value);
            eprintln!("# {}", s.source);
        }
        "device.key_path" => {
            let s = effective.key_path.as_ref().ok_or_else(|| {
                anyhow::anyhow!(
                    "Key 'device.key_path' is not set in '{mapper_dir}/mapper.toml' \
                     and is not configured in tedge.toml"
                )
            })?;
            println!("{}", s.value);
            eprintln!("# {}", s.source);
        }
        "device.root_cert_path" => {
            let s = &effective.root_cert_path;
            println!("{}", s.value);
            eprintln!("# {}", s.source);
        }
        _ => unreachable!("unexpected schema key: {key}"),
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use tedge_test_utils::fs::TempTedgeDir;

    // Test EC certificate (CN = "localhost") and matching private key.
    const TEST_CERT_PEM: &str = "\
-----BEGIN CERTIFICATE-----\n\
MIIBnzCCAUWgAwIBAgIUSTUtJUfUdERMKBwsfdRv9IbvQicwCgYIKoZIzj0EAwIw\n\
FDESMBAGA1UEAwwJbG9jYWxob3N0MCAXDTIzMTExNDE2MDUwOVoYDzMwMjMwMzE3\n\
MTYwNTA5WjAUMRIwEAYDVQQDDAlsb2NhbGhvc3QwWTATBgcqhkjOPQIBBggqhkjO\n\
PQMBBwNCAAR2SVEPD34AAxFuk0xYm60p7hA7+1SW+sFHazBRg32ifFd0o2Mn+Tf+\n\
voYflBi3v4lhr361RoWB8QfmaGN05vv+o3MwcTAdBgNVHQ4EFgQUAb4jQ7RQ/xyg\n\
cZM+We8ik29/oxswHwYDVR0jBBgwFoAUAb4jQ7RQ/xygcZM+We8ik29/oxswIQYD\n\
VR0RBBowGIIJbG9jYWxob3N0ggsqLmxvY2FsaG9zdDAMBgNVHRMBAf8EAjAAMAoG\n\
CCqGSM49BAMCA0gAMEUCIA6QrxoDHQJqoly7d8VN0sj0eDvfFpbbZdSnzBd6R8AP\n\
AiEAm/PAH3IPGuHRBIpdC0rNR8F/l3WcN9I9984qKZdG5rs=\n\
-----END CERTIFICATE-----\n";

    const TEST_KEY_PEM: &str = "\
-----BEGIN EC PRIVATE KEY-----\n\
MHcCAQEEIBX2Z/NKGEX14QbH4kb5GXom0pqSPfX0mxdWbLb86apEoAoGCCqGSM49\n\
AwEHoUQDQgAEdklRDw9+AAMRbpNMWJutKe4QO/tUlvrBR2swUYN9onxXdKNjJ/k3\n\
/r6GH5QYt7+JYa9+tUaFgfEH5mhjdOb7/g==\n\
-----END EC PRIVATE KEY-----\n";

    mod split_key {
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
    }

    mod walk_toml {
        use super::*;

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

    mod list_mappers {
        use super::*;

        #[tokio::test]
        async fn empty_mappers_dir_returns_empty() {
            let ttd = TempTedgeDir::new();
            let mappers_root = ttd.utf8_path().join("mappers");
            tokio::fs::create_dir_all(&mappers_root).await.unwrap();

            assert!(scan_mappers_shallow(&mappers_root).await.is_empty());
        }

        #[tokio::test]
        async fn dir_without_mapper_toml_or_flows_is_included() {
            let ttd = TempTedgeDir::new();
            let mappers_root = ttd.utf8_path().join("mappers");
            tokio::fs::create_dir_all(mappers_root.join("mymapper"))
                .await
                .unwrap();

            let mappers = scan_mappers_shallow(&mappers_root).await;
            let names: Vec<_> = mappers.iter().map(|(n, _)| n.as_str()).collect();
            assert_eq!(names, vec!["mymapper"]);
        }

        #[tokio::test]
        async fn flows_only_mapper_is_included() {
            let ttd = TempTedgeDir::new();
            let mappers_root = ttd.utf8_path().join("mappers");
            let flows_dir = mappers_root.join("thingsboard/flows");
            tokio::fs::create_dir_all(&flows_dir).await.unwrap();

            let mappers = scan_mappers_shallow(&mappers_root).await;
            let names: Vec<_> = mappers.iter().map(|(n, _)| n.as_str()).collect();
            assert_eq!(names, vec!["thingsboard"]);
        }

        #[tokio::test]
        async fn lists_mapper_with_cloud_type_in_table() {
            let ttd = TempTedgeDir::new();
            let mappers_root = ttd.utf8_path().join("mappers");
            let c8y_dir = mappers_root.join("c8y");
            tokio::fs::create_dir_all(&c8y_dir).await.unwrap();
            tokio::fs::write(c8y_dir.join("mapper.toml"), "cloud_type = \"c8y\"\n")
                .await
                .unwrap();

            let mappers = scan_mappers_shallow(&mappers_root).await;
            assert_eq!(mappers.len(), 1);
            assert_eq!(mappers[0].0, "c8y");
            let table = mappers[0].1.as_ref().unwrap();
            assert_eq!(
                table.get("cloud_type").and_then(|v| v.as_str()),
                Some("c8y")
            );
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

            let mappers = scan_mappers_shallow(&mappers_root).await;
            assert_eq!(mappers.len(), 1);
            assert_eq!(mappers[0].0, "thingsboard");
        }

        #[tokio::test]
        async fn lists_mixed_mappers_sorted() {
            let ttd = TempTedgeDir::new();
            let mappers_root = ttd.utf8_path().join("mappers");
            for name in ["zz-mapper", "aa-mapper", "mm-mapper"] {
                tokio::fs::create_dir_all(mappers_root.join(name))
                    .await
                    .unwrap();
            }
            let mappers = scan_mappers_shallow(&mappers_root).await;
            let names: Vec<_> = mappers.iter().map(|(n, _)| n.as_str()).collect();
            assert_eq!(names, vec!["aa-mapper", "mm-mapper", "zz-mapper"]);
        }

        #[tokio::test]
        async fn cert_cn_shown_with_tag_in_device_id_column() {
            let ttd = TempTedgeDir::new();
            let mappers_root = ttd.utf8_path().join("mappers");
            let mapper_dir = mappers_root.join("tb");
            tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
            let cert = mapper_dir.join("cert.pem");
            let key = mapper_dir.join("key.pem");
            tokio::fs::write(&cert, TEST_CERT_PEM).await.unwrap();
            tokio::fs::write(&key, TEST_KEY_PEM).await.unwrap();
            tokio::fs::write(
                mapper_dir.join("mapper.toml"),
                format!(
                    "url = \"mqtt.example.com:8883\"\n\
                     [device]\ncert_path = \"{cert}\"\nkey_path = \"{key}\"\n"
                ),
            )
            .await
            .unwrap();

            let mappers = scan_mappers_shallow(&mappers_root).await;
            let tedge_config = tedge_config::TEdgeConfig::load_toml_str("");
            let rows = build_mapper_rows(&mappers_root, &mappers, &tedge_config).await;

            assert_eq!(rows.len(), 1);
            assert_eq!(
                rows[0].device_id, "localhost [cert CN]",
                "device_id should show CN with [cert CN] tag"
            );
        }

        #[tokio::test]
        async fn tedge_toml_device_id_shown_with_tag() {
            let ttd = TempTedgeDir::new();
            let mappers_root = ttd.utf8_path().join("mappers");
            let mapper_dir = mappers_root.join("tb");
            tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
            let creds = ttd.utf8_path().join("creds.toml");
            tokio::fs::write(
                &creds,
                "[credentials]\nusername = \"u\"\npassword = \"p\"\n",
            )
            .await
            .unwrap();
            tokio::fs::write(
                mapper_dir.join("mapper.toml"),
                format!("url = \"mqtt.example.com:8883\"\ncredentials_path = \"{creds}\"\n"),
            )
            .await
            .unwrap();

            let mappers = scan_mappers_shallow(&mappers_root).await;
            let tedge_config =
                tedge_config::TEdgeConfig::load_toml_str("device.id = \"root-device\"");
            let rows = build_mapper_rows(&mappers_root, &mappers, &tedge_config).await;

            assert_eq!(rows.len(), 1);
            assert_eq!(
                rows[0].device_id, "root-device [tedge.toml]",
                "device_id should show tedge.toml value with [tedge.toml] tag"
            );
        }

        #[tokio::test]
        async fn unreadable_cert_leaves_device_id_blank() {
            let ttd = TempTedgeDir::new();
            let mappers_root = ttd.utf8_path().join("mappers");
            let mapper_dir = mappers_root.join("tb");
            tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
            tokio::fs::write(
                mapper_dir.join("mapper.toml"),
                "url = \"mqtt.example.com:8883\"\n\
                 [device]\ncert_path = \"/nonexistent/cert.pem\"\nkey_path = \"/nonexistent/key.pem\"\n",
            )
            .await
            .unwrap();

            let mappers = scan_mappers_shallow(&mappers_root).await;
            let tedge_config = tedge_config::TEdgeConfig::load_toml_str("");
            let rows = build_mapper_rows(&mappers_root, &mappers, &tedge_config).await;

            // Command must not fail — the mapper is still listed
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].name, "tb");
            assert!(
                rows[0].device_id.is_empty(),
                "device_id should be blank for unreadable cert, got: {:?}",
                rows[0].device_id
            );
        }

        #[tokio::test]
        async fn flows_only_mapper_has_blank_url_and_identity() {
            let ttd = TempTedgeDir::new();
            let mappers_root = ttd.utf8_path().join("mappers");
            tokio::fs::create_dir_all(mappers_root.join("thingsboard/flows"))
                .await
                .unwrap();

            let mappers = scan_mappers_shallow(&mappers_root).await;
            let tedge_config = tedge_config::TEdgeConfig::load_toml_str("");
            let rows = build_mapper_rows(&mappers_root, &mappers, &tedge_config).await;

            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].name, "thingsboard");
            assert!(
                rows[0].url.is_empty(),
                "url should be blank for flows-only mapper"
            );
            assert!(
                rows[0].device_id.is_empty(),
                "device_id should be blank for flows-only mapper"
            );
            assert!(
                rows[0].cloud_type.is_empty(),
                "cloud_type should be blank for flows-only mapper"
            );
        }

        #[tokio::test]
        async fn cloud_type_shown_for_mappers_that_set_it() {
            let ttd = TempTedgeDir::new();
            let mappers_root = ttd.utf8_path().join("mappers");
            // c8y and production have cloud_type; thingsboard does not
            for (name, content) in [
                ("c8y", "cloud_type = \"c8y\"\n"),
                ("production", "cloud_type = \"c8y\"\n"),
                ("thingsboard", "url = \"mqtt.tb.io:8883\"\n"),
            ] {
                let dir = mappers_root.join(name);
                tokio::fs::create_dir_all(&dir).await.unwrap();
                tokio::fs::write(dir.join("mapper.toml"), content)
                    .await
                    .unwrap();
            }

            let mappers = scan_mappers_shallow(&mappers_root).await;
            let tedge_config = tedge_config::TEdgeConfig::load_toml_str("");
            let rows = build_mapper_rows(&mappers_root, &mappers, &tedge_config).await;

            assert_eq!(rows.len(), 3);
            let by_name: std::collections::HashMap<_, _> =
                rows.iter().map(|r| (r.name.as_str(), r)).collect();
            assert_eq!(by_name["c8y"].cloud_type, "c8y");
            assert_eq!(by_name["production"].cloud_type, "c8y");
            assert!(
                by_name["thingsboard"].cloud_type.is_empty(),
                "thingsboard should have no cloud_type"
            );
        }
    }

    mod config_get {
        use super::*;
        use tedge_config::TEdgeConfig;

        async fn write_cert(dir: &camino::Utf8Path) -> (camino::Utf8PathBuf, camino::Utf8PathBuf) {
            let cert = dir.join("cert.pem");
            let key = dir.join("key.pem");
            tokio::fs::write(&cert, TEST_CERT_PEM).await.unwrap();
            tokio::fs::write(&key, TEST_KEY_PEM).await.unwrap();
            (cert, key)
        }

        /// Helper: runs a config-get for `<mapper_name>.<key>` against `mappers_root`.
        async fn run_get(
            mappers_root: &camino::Utf8Path,
            mapper_name: &str,
            key: &str,
            tedge_config: TEdgeConfig,
        ) -> Result<(), anyhow::Error> {
            let cmd = MapperConfigGetCommand {
                mappers_root: mappers_root.to_owned(),
                mapper_name: mapper_name.to_string(),
                toml_key: key.to_string(),
            };
            cmd.execute(tedge_config).await.map_err(|e| match e {
                MaybeFancy::Unfancy(e) => e,
                MaybeFancy::Fancy(f) => anyhow::anyhow!("{f}"),
            })
        }

        #[tokio::test]
        async fn url_returns_effective_value() {
            let ttd = TempTedgeDir::new();
            let mappers_root = ttd.utf8_path().join("mappers");
            let mapper_dir = mappers_root.join("tb");
            tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
            tokio::fs::write(
                mapper_dir.join("mapper.toml"),
                "url = \"mqtt.example.com:8883\"\n",
            )
            .await
            .unwrap();

            // Just ensure it doesn't error and we can call it
            let result = run_get(
                &mappers_root,
                "tb",
                "url",
                TEdgeConfig::load_toml_str("device.id = \"test\""),
            )
            .await;
            assert!(result.is_ok(), "config get url should succeed: {result:?}");
        }

        #[tokio::test]
        async fn device_id_inferred_from_cert_cn() {
            let ttd = TempTedgeDir::new();
            let mappers_root = ttd.utf8_path().join("mappers");
            let mapper_dir = mappers_root.join("tb");
            tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
            let (cert, key) = write_cert(ttd.utf8_path()).await;
            tokio::fs::write(
                mapper_dir.join("mapper.toml"),
                "url = \"mqtt.example.com:8883\"\n",
            )
            .await
            .unwrap();
            let tedge_config = TEdgeConfig::load_toml_str(&format!(
                "device.cert_path = \"{cert}\"\ndevice.key_path = \"{key}\"\n"
            ));

            let result = run_get(&mappers_root, "tb", "device.id", tedge_config).await;
            assert!(
                result.is_ok(),
                "device.id should be resolved from cert CN: {result:?}"
            );
        }

        #[tokio::test]
        async fn device_id_falls_back_to_tedge_toml() {
            let ttd = TempTedgeDir::new();
            let mappers_root = ttd.utf8_path().join("mappers");
            let mapper_dir = mappers_root.join("tb");
            tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
            // Password auth — no cert, tedge.toml has device.id
            let creds = ttd.utf8_path().join("creds.toml");
            tokio::fs::write(
                &creds,
                "[credentials]\nusername = \"u\"\npassword = \"p\"\n",
            )
            .await
            .unwrap();
            tokio::fs::write(
                mapper_dir.join("mapper.toml"),
                format!("url = \"mqtt.example.com:8883\"\ncredentials_path = \"{creds}\"\n"),
            )
            .await
            .unwrap();
            let tedge_config = TEdgeConfig::load_toml_str("device.id = \"root-device\"");

            let result = run_get(&mappers_root, "tb", "device.id", tedge_config).await;
            assert!(
                result.is_ok(),
                "device.id should fall back to tedge.toml: {result:?}"
            );
        }

        #[tokio::test]
        async fn device_id_errors_when_cert_unreadable() {
            let ttd = TempTedgeDir::new();
            let mappers_root = ttd.utf8_path().join("mappers");
            let mapper_dir = mappers_root.join("tb");
            tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
            tokio::fs::write(
                mapper_dir.join("mapper.toml"),
                "url = \"mqtt.example.com:8883\"\n",
            )
            .await
            .unwrap();
            // Cert path set but file doesn't exist
            let tedge_config = TEdgeConfig::load_toml_str(
                "device.cert_path = \"/nonexistent/cert.pem\"\n\
                 device.key_path = \"/nonexistent/key.pem\"\n",
            );

            let result = run_get(&mappers_root, "tb", "device.id", tedge_config).await;
            assert!(result.is_err(), "should error when cert unreadable");
            let msg = format!("{}", result.unwrap_err());
            assert!(
                msg.contains("unreadable") || msg.contains("certificate"),
                "error should mention certificate: {msg}"
            );
        }

        #[tokio::test]
        async fn relative_cert_path_resolved_to_absolute() {
            let ttd = TempTedgeDir::new();
            let mappers_root = ttd.utf8_path().join("mappers");
            let mapper_dir = mappers_root.join("tb");
            tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
            // Write cert files in mapper_dir so relative path resolves
            tokio::fs::write(mapper_dir.join("cert.pem"), TEST_CERT_PEM)
                .await
                .unwrap();
            tokio::fs::write(mapper_dir.join("key.pem"), TEST_KEY_PEM)
                .await
                .unwrap();
            tokio::fs::write(
                mapper_dir.join("mapper.toml"),
                "url = \"mqtt.example.com:8883\"\n\
                 [device]\ncert_path = \"cert.pem\"\nkey_path = \"key.pem\"\n",
            )
            .await
            .unwrap();

            let result = run_get(
                &mappers_root,
                "tb",
                "device.cert_path",
                TEdgeConfig::load_toml_str(""),
            )
            .await;
            assert!(
                result.is_ok(),
                "relative cert_path should resolve: {result:?}"
            );
        }

        #[tokio::test]
        async fn missing_mapper_errors_with_available_list() {
            let ttd = TempTedgeDir::new();
            let mappers_root = ttd.utf8_path().join("mappers");
            tokio::fs::create_dir_all(mappers_root.join("existing"))
                .await
                .unwrap();

            let result = run_get(
                &mappers_root,
                "nonexistent",
                "url",
                TEdgeConfig::load_toml_str(""),
            )
            .await;
            assert!(result.is_err());
            let msg = format!("{}", result.unwrap_err());
            assert!(
                msg.contains("existing"),
                "error should list available mappers: {msg}"
            );
        }

        #[tokio::test]
        async fn custom_key_returns_raw_toml_value() {
            let ttd = TempTedgeDir::new();
            let mappers_root = ttd.utf8_path().join("mappers");
            let mapper_dir = mappers_root.join("tb");
            tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
            tokio::fs::write(
                mapper_dir.join("mapper.toml"),
                "url = \"mqtt.example.com:8883\"\n[bridge]\ntopic_prefix = \"tb\"\n",
            )
            .await
            .unwrap();

            let result = run_get(
                &mappers_root,
                "tb",
                "bridge.topic_prefix",
                TEdgeConfig::load_toml_str(""),
            )
            .await;
            assert!(
                result.is_ok(),
                "custom key passthrough should succeed: {result:?}"
            );
        }

        #[tokio::test]
        async fn mapper_without_toml_errors() {
            let ttd = TempTedgeDir::new();
            let mappers_root = ttd.utf8_path().join("mappers");
            let mapper_dir = mappers_root.join("tb");
            tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
            // No mapper.toml written

            let result = run_get(&mappers_root, "tb", "url", TEdgeConfig::load_toml_str("")).await;
            assert!(result.is_err(), "should error when mapper.toml is absent");
            let msg = format!("{}", result.unwrap_err());
            assert!(
                msg.contains("mapper.toml"),
                "error should mention mapper.toml: {msg}"
            );
        }

        #[tokio::test]
        async fn missing_key_errors() {
            let ttd = TempTedgeDir::new();
            let mappers_root = ttd.utf8_path().join("mappers");
            let mapper_dir = mappers_root.join("tb");
            tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
            tokio::fs::write(
                mapper_dir.join("mapper.toml"),
                "url = \"mqtt.example.com:8883\"\n",
            )
            .await
            .unwrap();

            let result = run_get(
                &mappers_root,
                "tb",
                "nonexistent.key",
                TEdgeConfig::load_toml_str(""),
            )
            .await;
            assert!(result.is_err(), "should error when key is not found");
            let msg = format!("{}", result.unwrap_err());
            assert!(
                msg.contains("nonexistent.key") || msg.contains("not found"),
                "error should mention the missing key: {msg}"
            );
        }
    }
}
