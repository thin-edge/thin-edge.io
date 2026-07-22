use anyhow::Context;
use tedge_config::tedge_toml::CloudConfig;
use tedge_config::TEdgeConfig;
use tedge_p11::CryptokiConfig;

use crate::command::Command;
use crate::log::MaybeFancy;
use crate::ConfigError;

/// Arguments of the token-listing command.
#[derive(Debug, clap::Args)]
pub struct ListTokensArgs {}

impl ListTokensArgs {
    pub fn build_command(self, config: &TEdgeConfig) -> Result<Box<dyn Command>, ConfigError> {
        // Listing enumerates every token exposed by the module, so it is not scoped to a cloud.
        let cryptoki_config = config
            .device
            .cryptoki_config(None::<&dyn CloudConfig>)?
            .context("Cryptoki config is not enabled")?;

        Ok(ListTokensCmd { cryptoki_config }.into_boxed())
    }
}

pub struct ListTokensCmd {
    pub cryptoki_config: CryptokiConfig,
}

#[async_trait::async_trait]
impl Command for ListTokensCmd {
    fn description(&self) -> String {
        "List the PKCS #11 tokens available to the HSM.".into()
    }

    async fn execute(&self, _config: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        let cryptoki = tedge_p11::tedge_p11_service(self.cryptoki_config.clone())?;
        let response = cryptoki.list_tokens()?;

        if response.tokens.is_empty() {
            eprintln!("No PKCS #11 tokens were found. Ensure the HSM is connected.");
            return Ok(());
        }

        for (i, token) in response.tokens.iter().enumerate() {
            if i > 0 {
                println!();
            }
            let label = if token.label.is_empty() {
                "<uninitialized>"
            } else {
                &token.label
            };
            println!("Slot {}", token.slot);
            println!("  Label:        {label}");
            println!(
                "  Initialized:  {}",
                if token.initialized { "yes" } else { "no" }
            );
            println!("  Model:        {}", token.model);
            println!("  Manufacturer: {}", token.manufacturer);
            println!("  Serial:       {}", token.serial);
            println!("  URI:          {}", token.uri);
        }

        Ok(())
    }
}
