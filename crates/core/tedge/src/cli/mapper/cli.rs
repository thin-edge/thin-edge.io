use crate::cli::common::mapper_config_key_completions;
use crate::command::BuildCommand;
use crate::command::Command;
use crate::log::MaybeFancy;
use crate::ConfigError;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use clap_complete::ArgValueCandidates;
use tedge_config::TEdgeConfig;
use tedge_mapper::custom_mapper_config::load_mapper_config;
use tedge_mapper::custom_mapper_config::scan_mappers_shallow;
use tedge_mapper::custom_mapper_resolve::resolve_effective_config;
use yansi::Paint;

#[derive(clap::Subcommand, Debug)]
pub enum MapperCli {
    /// List available mappers and their cloud type
    List,

    /// Read a value from a mapper's config (hidden alias for `tedge config get mappers.*`)
    #[clap(hide = true)]
    Config {
        #[clap(subcommand)]
        cmd: MapperConfigCmd,
    },
}

#[derive(clap::Subcommand, Debug)]
pub enum MapperConfigCmd {
    /// Get a config value — delegates to `tedge config get mappers.<key>`
    Get {
        /// The key to look up, e.g. `thingsboard.url`
        #[arg(add(ArgValueCandidates::new(mapper_config_key_completions)))]
        key: String,
    },
}

#[async_trait::async_trait]
impl BuildCommand for MapperCli {
    async fn build_command(self, config: &TEdgeConfig) -> Result<Box<dyn Command>, ConfigError> {
        let mappers_root = config.root_dir().join("mappers");
        match self {
            MapperCli::List => Ok(ListMappersCommand { mappers_root }.into_boxed()),
            #[cfg(feature = "mapper-config")]
            MapperCli::Config {
                cmd: MapperConfigCmd::Get { key },
            } => {
                use crate::cli::config::MapperGetConfigCommand;
                Ok(MapperGetConfigCommand {
                    key: format!("mappers.{key}"),
                }
                .into_boxed())
            }
            #[cfg(not(feature = "mapper-config"))]
            MapperCli::Config { .. } => {
                Err(anyhow::anyhow!(
                    "tedge mapper config requires the 'mapper-config' feature"
                )
                .into())
            }
        }
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
        tedge_mapper::warn_misconfigured_mapper_dirs(&self.mappers_root).await;
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

#[cfg(test)]
mod tests {
    use super::*;
    use tedge_test_utils::fs::TempTedgeDir;

    mod format_config_set_cmd_fn {
        use tedge_config::cli::format_config_set_cmd;

        #[test]
        fn bare_mapper_name() {
            assert_eq!(
                format_config_set_cmd("c8y", "topic_prefix"),
                "tedge config set c8y.topic_prefix <value>"
            );
        }

        #[test]
        fn profile_qualified_mapper_name() {
            assert_eq!(
                format_config_set_cmd("c8y.prod", "mqtt.port"),
                "tedge config set c8y.mqtt.port <value> --profile prod"
            );
        }

        #[test]
        fn nested_toml_key() {
            assert_eq!(
                format_config_set_cmd("az", "device.cert_path"),
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

    #[allow(dead_code)]
    const TEST_KEY_PEM: &str = "\
-----BEGIN EC PRIVATE KEY-----\n\
MHcCAQEEIBX2Z/NKGEX14QbH4kb5GXom0pqSPfX0mxdWbLb86apEoAoGCCqGSM49\n\
AwEHoUQDQgAEdklRDw9+AAMRbpNMWJutKe4QO/tUlvrBR2swUYN9onxXdKNjJ/k3\n\
/r6GH5QYt7+JYa9+tUaFgfEH5mhjdOb7/g==\n\
-----END EC PRIVATE KEY-----\n";

    mod list_mappers {
        use super::*;

        #[tokio::test]
        async fn empty_mappers_dir_returns_empty() {
            let ttd = TempTedgeDir::new();
            let mappers_root = ttd.path().join("mappers");
            tokio::fs::create_dir_all(&mappers_root).await.unwrap();

            assert!(scan_mappers_shallow(&mappers_root).await.is_empty());
        }

        #[tokio::test]
        async fn dir_without_mapper_toml_or_flows_is_included() {
            let ttd = TempTedgeDir::new();
            let mappers_root = ttd.path().join("mappers");
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
            let mappers_root = ttd.path().join("mappers");
            let flows_dir = mappers_root.join("thingsboard/flows");
            tokio::fs::create_dir_all(&flows_dir).await.unwrap();

            let mappers = scan_mappers_shallow(&mappers_root).await;
            let names: Vec<_> = mappers.iter().map(|(n, _)| n.as_str()).collect();
            assert_eq!(names, vec!["thingsboard"]);
        }

        #[tokio::test]
        async fn lists_mapper_with_cloud_type_in_table() {
            let ttd = TempTedgeDir::new();
            let mappers_root = ttd.path().join("mappers");
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
            let mappers_root = ttd.path().join("mappers");
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
            let mappers_root = ttd.path().join("mappers");
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
            let mappers_root = ttd.path().join("mappers");
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
            let mappers_root = ttd.path().join("mappers");
            let mapper_dir = mappers_root.join("tb");
            tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
            let creds = ttd.path().join("creds.toml");
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
            let mappers_root = ttd.path().join("mappers");
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
            let mappers_root = ttd.path().join("mappers");
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
            let mappers_root = ttd.path().join("mappers");
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
}