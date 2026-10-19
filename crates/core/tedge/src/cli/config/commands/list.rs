use crate::command::Command;
use crate::log::MaybeFancy;
use pad::PadStr;
use std::io::stdout;
use std::io::IsTerminal;
use tedge_config::tedge_toml::READABLE_KEYS;
use tedge_config::TEdgeConfig;
use yansi::Paint;

pub struct ListConfigCommand {
    pub is_all: bool,
    pub is_doc: bool,
    pub filter: Option<String>,
}

#[async_trait::async_trait]
impl Command for ListConfigCommand {
    fn description(&self) -> String {
        "list the configuration keys and values".into()
    }

    async fn execute(&self, tedge_config: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        if self.is_doc {
            print_config_doc(&tedge_config, self.filter.as_deref()).await;
        } else {
            print_config_list(&tedge_config, self.is_all, self.filter.as_deref()).await?;
        }

        Ok(())
    }
}

async fn print_config_list(
    config: &TEdgeConfig,
    all: bool,
    filter: Option<&str>,
) -> Result<(), anyhow::Error> {
    let mut keys_without_values = Vec::new();
    for config_key in config.readable_keys() {
        if !key_matches_filter(&config_key.to_cow_str(), filter) {
            continue;
        }
        match config.read_string(&config_key).ok() {
            Some(value) => {
                println!("{config_key}={value}");
            }
            None => {
                keys_without_values.push(config_key.to_string());
            }
        }
    }
    #[cfg(feature = "mapper-config")]
    {
        let mappers = list_mapper_config(&config.config_root(), filter).await;
        for line in mappers.values {
            println!("{line}");
        }
        keys_without_values.extend(mappers.keys_without_values);
        for warning in mappers.warnings {
            tracing::warn!("{warning}");
        }
    }

    if all && !keys_without_values.is_empty() {
        println!();
        for key in keys_without_values {
            println!("{key}=");
        }
    }
    Ok(())
}

struct DocEntry {
    key: String,
    doc: String,
    note: Option<String>,
    examples: Vec<String>,
}

/// The custom mapper part of `tedge config list`
#[cfg(feature = "mapper-config")]
#[derive(Default)]
struct MapperListing {
    /// `key=value` lines
    values: Vec<String>,
    keys_without_values: Vec<String>,
    /// One per mapper whose configuration could not be read
    warnings: Vec<String>,
}

#[cfg(feature = "mapper-config")]
async fn list_mapper_config(
    config_root: &tedge_utils::paths::TedgePaths,
    filter: Option<&str>,
) -> MapperListing {
    let mut listing = MapperListing::default();
    let fed = match tedge_mapper_config::load_federated_config(config_root).await {
        Ok(fed) => fed,
        Err(e) => {
            listing
                .warnings
                .push(format!("Failed to load mapper configuration: {e}"));
            return listing;
        }
    };
    let mut read_errors = std::collections::BTreeSet::new();
    for entry in fed.all_entries() {
        if !entry.key.starts_with("mappers.") {
            continue;
        }
        if is_builtin_cloud_mapper_key(&entry.key) {
            continue;
        }
        if !key_matches_filter(&entry.key, filter) {
            continue;
        }
        match fed.read(&entry.key) {
            Ok(Some(value)) => listing
                .values
                .push(format!("{key}={value}", key = entry.key)),
            Ok(None) => listing.keys_without_values.push(entry.key),
            // Every key of a mapper whose file didn't load fails the same way
            Err(e) => {
                read_errors.insert(e.to_string());
            }
        }
    }
    // Each error already says what failed and on which file
    listing.warnings.extend(read_errors);
    listing
}

fn normalise_doc_comment(comment: &str) -> String {
    let mut normalised = comment.replace('\n', " ");
    if !normalised.ends_with('.') {
        normalised.push('.');
    }
    normalised.push(' ');
    normalised
}

async fn print_config_doc(config: &TEdgeConfig, filter: Option<&str>) {
    #[cfg_attr(not(feature = "mapper-config"), expect(unused_mut))]
    let mut entries: Vec<DocEntry> = READABLE_KEYS
        .iter()
        .filter(|(key, _)| key_matches_filter(key, filter))
        .map(|(key, ty)| {
            let doc = ty.comment.map(normalise_doc_comment).unwrap_or_default();

            let note = ty.metas.get("note").map(|s| s.to_string());

            let examples = match ty.example {
                Some(doku::Example::Simple(val)) | Some(doku::Example::Literal(val)) => {
                    vec![val.to_string()]
                }
                Some(doku::Example::Compound(vals)) => vals.iter().map(|s| s.to_string()).collect(),
                None => vec![],
            };

            DocEntry {
                key: key.to_string(),
                doc,
                note,
                examples,
            }
        })
        .collect();

    #[cfg(feature = "mapper-config")]
    match tedge_mapper_config::load_federated_config(&config.config_root()).await {
        Ok(fed) => {
            for entry in fed.all_entries() {
                if !entry.key.starts_with("mappers.") {
                    continue;
                }
                if is_builtin_cloud_mapper_key(&entry.key) {
                    continue;
                }
                if !key_matches_filter(&entry.key, filter) {
                    continue;
                }
                let doc = if entry.doc.is_empty() {
                    String::new()
                } else {
                    normalise_doc_comment(&entry.doc.join(" "))
                };
                let examples = entry.examples.into_iter().map(String::from).collect();
                entries.push(DocEntry {
                    key: entry.key,
                    doc,
                    note: None,
                    examples,
                });
            }
        }
        Err(e) => {
            tracing::warn!("Failed to load mapper configuration: {e}");
        }
    }

    // Silence the unused-variable warning when the feature is off
    let _ = config;

    print_doc_entries(&entries);
}

fn print_doc_entries(entries: &[DocEntry]) {
    let max_length = entries
        .iter()
        .map(|e| e.key.len())
        .max()
        .unwrap_or_default();

    for entry in entries {
        println!(
            "{}  {}",
            entry
                .key
                .pad_to_width_with_alignment(max_length, pad::Alignment::Right)
                .yellow(),
            entry.doc.as_str().italic()
        );

        if let Some(note) = &entry.note {
            println!(
                "{}  {} {note}",
                "".pad_to_width(max_length),
                yansi::Paint::blue("Note:")
            );
        }

        if !entry.examples.is_empty() {
            let label = if entry.examples.len() == 1 {
                "Example:"
            } else {
                "Examples:"
            };
            println!(
                "{}  {} {}",
                "".pad_to_width(max_length),
                yansi::Paint::green(label),
                entry.examples.join(", ")
            );
        }

        if !stdout().is_terminal() {
            println!();
        }
    }
}

/// Built-in cloud mappers (c8y, az, aws) already appear under their own
/// top-level namespace (e.g. `c8y.url`), so we hide them from the
/// `mappers.*` listing to avoid showing every key twice.
#[cfg(feature = "mapper-config")]
fn is_builtin_cloud_mapper_key(key: &str) -> bool {
    const BUILTIN_CLOUDS: &[&str] = &["c8y", "az", "aws"];
    let Some(after) = key.strip_prefix("mappers.") else {
        return false;
    };
    BUILTIN_CLOUDS
        .iter()
        .any(|cloud| after == *cloud || after.starts_with(&format!("{cloud}.")))
}

fn key_matches_filter(key: &str, filter: Option<&str>) -> bool {
    match filter {
        Some(filter) => key.contains(filter),
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "mapper-config")]
    mod builtin_cloud_filter {
        use super::super::is_builtin_cloud_mapper_key;

        #[test]
        fn hides_builtin_clouds() {
            assert!(is_builtin_cloud_mapper_key("mappers.c8y.url"));
            assert!(is_builtin_cloud_mapper_key(
                "mappers.az.bridge.clean_session"
            ));
            assert!(is_builtin_cloud_mapper_key("mappers.aws.url"));
        }

        #[test]
        fn hides_builtin_cloud_profiles() {
            assert!(is_builtin_cloud_mapper_key("mappers.c8y.profiles.prod.url"));
        }

        #[test]
        fn shows_custom_mappers() {
            assert!(!is_builtin_cloud_mapper_key("mappers.thingsboard.url"));
            assert!(!is_builtin_cloud_mapper_key(
                "mappers.custom.bridge.clean_session"
            ));
        }

        #[test]
        fn ignores_non_mapper_keys() {
            assert!(!is_builtin_cloud_mapper_key("c8y.url"));
            assert!(!is_builtin_cloud_mapper_key("device.id"));
        }
    }

    #[cfg(feature = "mapper-config")]
    #[tokio::test]
    async fn a_broken_mapper_file_is_reported_once_while_other_mappers_are_listed() {
        let config_dir = tedge_test_utils::fs::TempTedgeDir::new();
        let mappers = config_dir.dir("mappers");
        mappers
            .dir("broken")
            .file("mapper.toml")
            .with_raw_content("[bridge\n");
        mappers
            .dir("thingsboard")
            .file("mapper.toml")
            .with_raw_content("url = \"tb.example.com:8883\"\n");

        let config_root =
            tedge_utils::paths::TedgePaths::from_root_with_defaults(config_dir.path(), "", "");
        let listing = list_mapper_config(&config_root, None).await;

        let broken_file = config_dir.path().join("mappers/broken/mapper.toml");
        assert_eq!(listing.warnings.len(), 1, "{:?}", listing.warnings);
        let warning = &listing.warnings[0];
        assert!(
            warning.starts_with(&format!("Failed to load {broken_file}: ")),
            "{warning}"
        );
        assert!(
            listing
                .values
                .contains(&"mappers.thingsboard.url=tb.example.com:8883".to_owned()),
            "{:?}",
            listing.values
        );
    }

    #[test]
    fn normalise_doc_comment_appends_period_and_space() {
        assert_eq!(normalise_doc_comment("hello"), "hello. ");
        assert_eq!(normalise_doc_comment("hello."), "hello. ");
        assert_eq!(normalise_doc_comment("line1\nline2"), "line1 line2. ");
    }
}
