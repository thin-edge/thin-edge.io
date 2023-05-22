use crate::command::Command;
use crate::ConfigError;
use pad::PadStr;
use tedge_config::new::ReadableKey;
use tedge_config::new::TEdgeConfig;
use tedge_config::new::READABLE_KEYS;

pub struct ListConfigCommand {
    pub is_all: bool,
    pub is_doc: bool,
    pub config: TEdgeConfig,
}

impl Command for ListConfigCommand {
    fn description(&self) -> String {
        "list the configuration keys and values".into()
    }

    fn execute(&self) -> anyhow::Result<()> {
        if self.is_doc {
            print_config_doc();
        } else {
            print_config_list(&self.config, self.is_all)?;
        }

        Ok(())
    }
}

fn print_config_list(config: &TEdgeConfig, all: bool) -> Result<(), ConfigError> {
    let mut keys_without_values = Vec::new();
    for config_key in ReadableKey::iter() {
        match config.read_string(config_key).ok() {
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

fn print_config_doc() {
    if atty::isnt(atty::Stream::Stdout) {
        yansi::Paint::disable();
    }

    let max_length = ReadableKey::iter()
        .map(|c| c.as_str().len())
        .max()
        .unwrap_or_default();

    for (key, ty) in READABLE_KEYS.iter() {
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
            yansi::Paint::yellow(
                key.pad_to_width_with_alignment(max_length, pad::Alignment::Right)
            ),
            yansi::Paint::default(docs).italic()
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

        if atty::isnt(atty::Stream::Stdout) {
            println!();
        }
    }
}
