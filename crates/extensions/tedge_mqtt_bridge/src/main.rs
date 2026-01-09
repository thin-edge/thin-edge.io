use anyhow::Context;
use ariadne::Color;
use ariadne::Label;
use ariadne::Report;
use ariadne::ReportKind;
use ariadne::Source;
use std::env;
use std::fs;
use tedge_config::TEdgeConfig;
use tedge_mqtt_bridge::config::ExpandError;
use tedge_mqtt_bridge::config::PersistedBridgeConfig;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.len() != 2 {
        anyhow::bail!("Usage: {} <path-to-toml-file>", args[0]);
    }

    let toml_path = &args[1];

    // Read and deserialize the TOML file
    let toml_content = fs::read_to_string(toml_path).context("Failed to read TOML file")?;

    let persisted_config: PersistedBridgeConfig = toml::from_str(&toml_content).map_err(|e| {
        print_toml_error(toml_path, &toml_content, &e);
        anyhow::anyhow!("Failed to deserialize TOML")
    })?;

    // Load TEdgeConfig from default location (/etc/tedge)
    let tedge_config = TEdgeConfig::load("/etc/tedge")
        .await
        .context("Failed to load TEdgeConfig")?;

    // Expand the bridge rules
    let expanded_rules = persisted_config.expand(&tedge_config).map_err(|e| {
        print_expansion_error(toml_path, &toml_content, &e);
        anyhow::anyhow!("Failed to expand bridge rules")
    })?;

    // Debug print the expanded rules
    println!("{:#?}", expanded_rules);

    Ok(())
}

fn print_toml_error(path: &str, source: &str, error: &toml::de::Error) {
    let span = error.span().unwrap_or(0..0);

    Report::build(ReportKind::Error, (path, span.clone()))
        .with_message("Failed to parse TOML configuration")
        .with_label(
            Label::new((path, span))
                .with_message(error.message())
                .with_color(Color::Red),
        )
        .finish()
        .eprint((path, Source::from(source)))
        .unwrap();
}

fn print_expansion_error(path: &str, source: &str, error: &ExpandError) {
    let mut report = Report::build(ReportKind::Error, (path, error.span.clone()))
        .with_message("Failed to expand bridge configuration")
        .with_label(
            Label::new((path, error.span.clone()))
                .with_message(&error.message)
                .with_color(Color::Red),
        );
    if let Some(help) = &error.help {
        report = report.with_note(help);
    }
    report
        .finish()
        .eprint((path, Source::from(source)))
        .unwrap();
}
