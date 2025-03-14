use crate::command::Command;
use crate::log::MaybeFancy;
use pad::PadStr;
use std::io::stdout;
use std::io::IsTerminal;
use tedge_config::tedge_toml::READABLE_KEYS;
use tedge_config::TEdgeConfig;
use tedge_config::TEdgeConfigLocation;
use yansi::Paint;

pub struct ListConfigCommand {
    pub is_all: bool,
    pub is_doc: bool,
    pub filter: Option<String>,
    pub config_location: TEdgeConfigLocation,
}

#[async_trait::async_trait]
impl Command for ListConfigCommand {
    fn description(&self) -> String {
        "list the configuration keys and values".into()
    }

    async fn execute(&self) -> Result<(), MaybeFancy<anyhow::Error>> {
        if self.is_doc {
            print_config_doc(self.filter.as_deref());
        } else {
            let config = self
                .config_location
                .load()
                .await
                .map_err(anyhow::Error::new)?;
            print_config_list(&config, self.is_all, self.filter.as_deref())?;
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
                println!("{}={}", config_key, value);
            }
            None => {
                keys_without_values.push(config_key);
            }
        }
    }
    if all && !keys_without_values.is_empty() {
        println!();
        for key in keys_without_values {
            println!("{}=", key);
        }
    }
    Ok(())
}

fn print_config_doc(filter: Option<&str>) {
    let max_length = READABLE_KEYS
        .iter()
        .filter(|(key, _)| key_matches_filter(key, filter))
        .map(|(key, _)| key.len())
        .max()
        .unwrap_or_default();

    for (key, ty) in READABLE_KEYS.iter() {
        if !key_matches_filter(key, filter) {
            continue;
        }
        let docs = ty
            .comment
            .map(|c| {
                let mut comment = c.replace('\n', " ");
                if !comment.ends_with('.') {
                    comment.push('.');
                };
                comment.push(' ');
                comment
            })
            .unwrap_or_default();

        println!(
            "{}  {}",
            key.pad_to_width_with_alignment(max_length, pad::Alignment::Right)
                .yellow(),
            docs.italic()
        );

        // TODO add a test to make sure people don't accidentally set the wrong meta name
        if let Some(note) = ty.metas.get("note") {
            println!(
                "{}  {} {note}",
                "".pad_to_width(max_length),
                yansi::Paint::blue("Note:")
            );
        }

        match ty.example {
            Some(doku::Example::Simple(val)) | Some(doku::Example::Literal(val)) => {
                println!(
                    "{}  {} {}",
                    "".pad_to_width(max_length),
                    yansi::Paint::green("Example:"),
                    val
                );
            }
            Some(doku::Example::Compound(val)) => {
                let vals = val
                    .iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                println!(
                    "{}  {} {}",
                    "".pad_to_width(max_length),
                    yansi::Paint::green("Examples:"),
                    vals
                );
            }
            None => (),
        };

        if !stdout().is_terminal() {
            println!();
        }
    }
}

fn key_matches_filter(key: &str, filter: Option<&str>) -> bool {
    match filter {
        Some(filter) => key.contains(filter),
        None => true,
    }
}
