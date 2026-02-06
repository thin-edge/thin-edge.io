use anyhow::Context;
use camino::Utf8Path;
use std::env;
use std::fs;
use tedge_config::TEdgeConfig;
use tedge_mqtt_bridge::AuthMethod;
use tedge_mqtt_bridge::BridgeConfig;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.len() != 2 {
        anyhow::bail!("Usage: {} <path-to-toml-file>", args[0]);
    }

    let toml_path = Utf8Path::new(&args[1]);

    // Read and deserialize the TOML file
    let toml_content = fs::read_to_string(toml_path).context("Failed to read TOML file")?;

    // Load TEdgeConfig from default location (/etc/tedge)
    let tedge_config = TEdgeConfig::load("/etc/tedge")
        .await
        .context("Failed to load TEdgeConfig")?;

    let mut bridge_config = BridgeConfig::new();
    bridge_config.add_rules_from_template(
        toml_path,
        &toml_content,
        &tedge_config,
        AuthMethod::Certificate,
        None,
    )?;

    // Debug print the expanded rules
    println!("{:#?}", bridge_config);

    Ok(())
}
