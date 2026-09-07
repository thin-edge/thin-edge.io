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
            print_config_doc(&tedge_config, self.filter.as_deref());
        } else {
            print_config_list(&tedge_config, self.is_all, self.filter.as_deref())?;
        }

        Ok(())
    }
}

fn print_config_list(
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
    match tedge_mapper_config::load_federated_config(config.root_dir()) {
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
                match fed.read(&entry.key).ok().flatten() {
                    Some(value) => println!("{key}={value}", key = entry.key),
                    None => keys_without_values.push(entry.key),
                }
            }
        }
        Err(e) => {
            tracing::warn!("Failed to load mapper configuration: {e}");
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

fn normalise_doc_comment(comment: &str) -> String {
    let mut normalised = comment.replace('\n', " ");
    if !normalised.ends_with('.') {
        normalised.push('.');
    }
    normalised.push(' ');
    normalised
}

fn print_config_doc(config: &TEdgeConfig, filter: Option<&str>) {
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
    match tedge_mapper_config::load_federated_config(config.root_dir()) {
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
    BUILTIN_CLOUDS.iter().any(|cloud| {
        after == *cloud
            || after.starts_with(&format!("{cloud}."))
    })
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
            assert!(is_builtin_cloud_mapper_key("mappers.az.bridge.clean_session"));
            assert!(is_builtin_cloud_mapper_key("mappers.aws.url"));
        }

        #[test]
        fn hides_builtin_cloud_profiles() {
            assert!(is_builtin_cloud_mapper_key("mappers.c8y.prod.url"));
        }

        #[test]
        fn shows_custom_mappers() {
            assert!(!is_builtin_cloud_mapper_key("mappers.thingsboard.url"));
            assert!(!is_builtin_cloud_mapper_key("mappers.custom.bridge.clean_session"));
        }

        #[test]
        fn ignores_non_mapper_keys() {
            assert!(!is_builtin_cloud_mapper_key("c8y.url"));
            assert!(!is_builtin_cloud_mapper_key("device.id"));
        }
    }

    #[test]
    fn normalise_doc_comment_appends_period_and_space() {
        assert_eq!(normalise_doc_comment("hello"), "hello. ");
        assert_eq!(normalise_doc_comment("hello."), "hello. ");
        assert_eq!(normalise_doc_comment("line1\nline2"), "line1 line2. ");
    }
}
