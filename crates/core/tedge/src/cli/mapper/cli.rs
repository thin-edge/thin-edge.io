use crate::cli::common::resolve_cloud;
use crate::cli::common::MaybeBorrowedCloud;
use crate::command::BuildCommand;
use crate::command::Command;
use crate::log::MaybeFancy;
use crate::ConfigError;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use tedge_config::TEdgeConfig;
use tedge_mapper::custom_mapper_config::load_mapper_config;
use tedge_mapper::custom_mapper_config::scan_mappers_shallow;
use tedge_mapper::custom_mapper_resolve::resolve_effective_config;
use tedge_mapper::custom_mapper_resolve::ConfigGetResult;
use tedge_mapper::custom_mapper_resolve::ConfigSource;
use tedge_mqtt_bridge::AuthMethod;
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
    for (name, _) in mappers {
        let mapper_dir = mappers_root.join(name);
        let (cloud_type, url, device_id) = match load_mapper_config(&mapper_dir).await {
            Ok(Some(raw)) => {
                let cloud_type = raw.cloud_type.map(|ct| ct.to_string()).unwrap_or_default();
                match resolve_effective_config(&raw, config, None, None).await {
                    Ok(effective) => {
                        let url = effective
                            .url
                            .map(|u| u.value.to_string())
                            .unwrap_or_default();
                        let device_id = effective
                            .device_id
                            .map(|d| format!("{} [{}]", d.value, d.source.short_tag()))
                            .unwrap_or_default();
                        (cloud_type, url, device_id)
                    }
                    Err(_) => (cloud_type, String::new(), String::new()),
                }
            }
            _ => (String::new(), String::new(), String::new()),
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

        for row in &rows {
            let name = &row.name;
            let cloud_type = &row.cloud_type;
            let url = &row.url;
            if row.device_id.is_empty() && row.url.is_empty() && row.cloud_type.is_empty() {
                println!("{}", name.yellow());
            } else {
                println!(
                    "{}\t{}\t{}\t{}",
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
        self.run(config, &mut std::io::stdout(), &mut std::io::stderr())
            .await
            .map_err(Into::into)
    }
}

impl MapperConfigGetCommand {
    async fn run(
        &self,
        config: TEdgeConfig,
        out: &mut impl std::io::Write,
        err: &mut impl std::io::Write,
    ) -> anyhow::Result<()> {
        let mapper_dir = self.mappers_root.join(&self.mapper_name);
        if !tokio::fs::try_exists(&mapper_dir).await.unwrap_or(false) {
            let available = scan_mappers_shallow(&self.mappers_root).await;
            let e: anyhow::Error = if available.is_empty() {
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
            return Err(e);
        }

        // For built-in mappers (c8y, aws, az) the effective config is derived from the
        // cloud reader in tedge.toml (with mapper.toml as an optional override), exactly
        // as the mapper does at runtime. For custom mappers mapper.toml is required.
        let effective = match resolve_cloud(&self.mapper_name, None) {
            #[cfg(feature = "c8y")]
            Some(MaybeBorrowedCloud::C8y(_)) => {
                tedge_mapper::c8y::mapper::resolve_effective_mapper_config(&config, None).await?
            }
            _ => {
                let raw = load_mapper_config(&mapper_dir).await?.ok_or_else(|| {
                    anyhow::anyhow!(
                        "Mapper '{}' has no mapper.toml at '{mapper_dir}'",
                        self.mapper_name
                    )
                })?;
                resolve_effective_config(&raw, &config, None, None).await?
            }
        };
        let sourced = match effective.get(&self.toml_key) {
            ConfigGetResult::Value(s) => s,
            ConfigGetResult::NotSet => {
                let e = if self.toml_key == "device.id"
                    && matches!(effective.effective_auth.value, AuthMethod::Certificate)
                {
                    anyhow::anyhow!(
                        "Cannot determine device.id for mapper '{}': \
                         certificate authentication is configured but the certificate is unreadable. \
                         Set 'device.id' explicitly in '{mapper_dir}/mapper.toml' to override.",
                        self.mapper_name
                    )
                } else {
                    anyhow::anyhow!(
                        "Key '{}' is not set for mapper '{}'",
                        self.toml_key,
                        self.mapper_name
                    )
                };
                return Err(e);
            }
            ConfigGetResult::UnknownKey => {
                return Err(anyhow::anyhow!(
                    "Unknown mapper config key: {}.{}",
                    self.mapper_name,
                    self.toml_key
                ));
            }
        };
        writeln!(out, "{}", sourced.value)?;
        writeln!(err, "# {}", sourced.source)?;
        if matches!(sourced.source, ConfigSource::TedgeConfig) {
            writeln!(
                err,
                "# To change this, use: {}",
                tedge_config_set_cmd(&self.mapper_name, &self.toml_key)
            )?;
        }

        Ok(())
    }
}

/// Formats a `tedge config set` command for a built-in mapper key.
///
/// The `mapper_name` may be bare (e.g. `"c8y"`) or profile-qualified
/// (e.g. `"c8y.prod"`). In the latter case the profile is passed via
/// `--profile` so the command is valid for the named profile.
fn tedge_config_set_cmd(mapper_name: &str, toml_key: &str) -> String {
    match mapper_name.split_once('.') {
        Some((cloud, profile)) => {
            format!("tedge config set {cloud}.{toml_key} <value> --profile {profile}")
        }
        None => format!("tedge config set {mapper_name}.{toml_key} <value>"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tedge_test_utils::fs::TempTedgeDir;

    mod tedge_config_set_cmd_fn {
        use super::*;

        #[test]
        fn bare_mapper_name() {
            assert_eq!(
                tedge_config_set_cmd("c8y", "topic_prefix"),
                "tedge config set c8y.topic_prefix <value>"
            );
        }

        #[test]
        fn profile_qualified_mapper_name() {
            assert_eq!(
                tedge_config_set_cmd("c8y.prod", "mqtt.port"),
                "tedge config set c8y.mqtt.port <value> --profile prod"
            );
        }

        #[test]
        fn nested_toml_key() {
            assert_eq!(
                tedge_config_set_cmd("az", "device.cert_path"),
                "tedge config set az.device.cert_path <value>"
            );
        }
    }

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
            let err = split_mapper_key("thingsboard").unwrap_err();
            assert_eq!(
                err.to_string(),
                "Invalid key 'thingsboard': expected format '<mapper-name>.<toml-key>', e.g. 'thingsboard.url'"
            );
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

        #[derive(Debug)]
        struct GetOutput {
            stdout: String,
            stderr: String,
        }

        /// Runs `tedge mapper config get <mapper_name>.<key>` and returns the captured
        /// stdout/stderr on success, or the error message on failure.
        async fn run_get(
            mappers_root: &camino::Utf8Path,
            mapper_name: &str,
            key: &str,
            tedge_config: TEdgeConfig,
        ) -> Result<GetOutput, anyhow::Error> {
            let cmd = MapperConfigGetCommand {
                mappers_root: mappers_root.to_owned(),
                mapper_name: mapper_name.to_string(),
                toml_key: key.to_string(),
            };
            let mut out = Vec::<u8>::new();
            let mut err = Vec::<u8>::new();
            cmd.run(tedge_config, &mut out, &mut err).await?;
            Ok(GetOutput {
                stdout: String::from_utf8(out).unwrap(),
                stderr: String::from_utf8(err).unwrap(),
            })
        }

        #[tokio::test]
        async fn url_prints_value_and_source() {
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

            let output = run_get(&mappers_root, "tb", "url", TEdgeConfig::load_toml_str(""))
                .await
                .unwrap();
            assert_eq!(output.stdout.trim(), "mqtt.example.com:8883");
            assert_eq!(output.stderr.trim(), "# from mapper.toml");
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

            let output = run_get(&mappers_root, "tb", "device.id", tedge_config)
                .await
                .unwrap();
            assert_eq!(output.stdout.trim(), "localhost");
            assert_eq!(
                output.stderr.trim(),
                format!("# inferred from certificate CN ({cert})")
            );
        }

        #[tokio::test]
        async fn device_id_falls_back_to_tedge_toml() {
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
            let tedge_config = TEdgeConfig::load_toml_str("device.id = \"root-device\"");

            let output = run_get(&mappers_root, "tb", "device.id", tedge_config)
                .await
                .unwrap();
            assert_eq!(output.stdout.trim(), "root-device");
            assert_eq!(
                output.stderr.trim(),
                "# not set in mapper.toml, inherited from tedge.toml"
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
            let tedge_config = TEdgeConfig::load_toml_str(
                "device.cert_path = \"/nonexistent/cert.pem\"\n\
                 device.key_path = \"/nonexistent/key.pem\"\n",
            );

            let result = run_get(&mappers_root, "tb", "device.id", tedge_config).await;
            let msg = format!("{}", result.unwrap_err());
            assert_eq!(
                msg,
                format!(
                    "Cannot determine device.id for mapper 'tb': \
                     certificate authentication is configured but the certificate is unreadable. \
                     Set 'device.id' explicitly in '{mapper_dir}/mapper.toml' to override."
                )
            );
        }

        #[tokio::test]
        async fn relative_cert_path_resolves_to_absolute_with_source() {
            let ttd = TempTedgeDir::new();
            let mappers_root = ttd.utf8_path().join("mappers");
            let mapper_dir = mappers_root.join("tb");
            tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
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

            let output = run_get(
                &mappers_root,
                "tb",
                "device.cert_path",
                TEdgeConfig::load_toml_str(""),
            )
            .await
            .unwrap();
            // stdout is the resolved absolute path
            assert!(
                output.stdout.trim().starts_with('/'),
                "cert_path should be absolute, got: {}",
                output.stdout.trim()
            );
            assert_eq!(
                output.stderr.trim(),
                "# relative path 'cert.pem' in mapper.toml, resolved to absolute"
            );
        }

        #[tokio::test]
        async fn url_not_set_errors() {
            let ttd = TempTedgeDir::new();
            let mappers_root = ttd.utf8_path().join("mappers");
            let mapper_dir = mappers_root.join("tb");
            tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
            // mapper.toml exists but has no url
            tokio::fs::write(
                mapper_dir.join("mapper.toml"),
                "[bridge]\ntopic_prefix = \"tb\"\n",
            )
            .await
            .unwrap();

            let msg = format!(
                "{}",
                run_get(&mappers_root, "tb", "url", TEdgeConfig::load_toml_str(""))
                    .await
                    .unwrap_err()
            );
            assert_eq!(msg, "Key 'url' is not set for mapper 'tb'");
        }

        #[tokio::test]
        async fn device_id_not_set_with_password_auth_errors() {
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
            // No device.id in tedge.toml and none in mapper.toml
            let msg = format!(
                "{}",
                run_get(
                    &mappers_root,
                    "tb",
                    "device.id",
                    TEdgeConfig::load_toml_str("")
                )
                .await
                .unwrap_err()
            );
            assert_eq!(msg, "Key 'device.id' is not set for mapper 'tb'");
        }

        #[tokio::test]
        async fn missing_mapper_lists_multiple_available() {
            let ttd = TempTedgeDir::new();
            let mappers_root = ttd.utf8_path().join("mappers");
            for name in ["alpha", "beta", "gamma"] {
                tokio::fs::create_dir_all(mappers_root.join(name))
                    .await
                    .unwrap();
            }

            let msg = format!(
                "{}",
                run_get(
                    &mappers_root,
                    "nonexistent",
                    "url",
                    TEdgeConfig::load_toml_str("")
                )
                .await
                .unwrap_err()
            );
            assert_eq!(
                msg,
                "Mapper 'nonexistent' not found. Available mappers: alpha, beta, gamma"
            );
        }

        #[tokio::test]
        async fn missing_mapper_no_mappers_configured() {
            let ttd = TempTedgeDir::new();
            let mappers_root = ttd.utf8_path().join("mappers");
            tokio::fs::create_dir_all(&mappers_root).await.unwrap();

            let result = run_get(
                &mappers_root,
                "nonexistent",
                "url",
                TEdgeConfig::load_toml_str(""),
            )
            .await;
            let msg = format!("{}", result.unwrap_err());
            assert_eq!(
                msg,
                format!(
                    "Mapper 'nonexistent' not found. No mappers configured under '{mappers_root}'."
                )
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
            let msg = format!("{}", result.unwrap_err());
            assert_eq!(
                msg,
                "Mapper 'nonexistent' not found. Available mappers: existing"
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
            let msg = format!("{}", result.unwrap_err());
            assert_eq!(
                msg,
                format!("Mapper 'tb' has no mapper.toml at '{mapper_dir}'")
            );
        }

        #[tokio::test]
        async fn schema_key_not_set_errors() {
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

            // credentials_path is a known schema key but is not set
            let result = run_get(
                &mappers_root,
                "tb",
                "credentials_path",
                TEdgeConfig::load_toml_str(""),
            )
            .await;
            let msg = format!("{}", result.unwrap_err());
            assert_eq!(msg, "Key 'credentials_path' is not set for mapper 'tb'");
        }

        #[tokio::test]
        async fn unknown_key_errors() {
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
            let msg = format!("{}", result.unwrap_err());
            assert_eq!(msg, "Unknown mapper config key: tb.nonexistent.key");
        }
    }
}
