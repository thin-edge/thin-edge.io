use anyhow::Context;
use tedge_config::tedge_toml::CloudConfig;
use tedge_config::TEdgeConfig;
use tedge_p11::service::ListKeysRequest;
use tedge_p11::CryptokiConfig;
use tedge_p11::SecretString;

use crate::command::Command;
use crate::log::MaybeFancy;
use crate::ConfigError;

/// Arguments of the key-listing command.
#[derive(Debug, clap::Args)]
pub struct ListKeysArgs {
    /// User PIN for logging into the token (required to list private keys).
    ///
    /// If omitted, the PIN configured for tedge-p11-server is used.
    #[arg(long)]
    pub pin: Option<String>,

    /// A PKCS #11 URI selecting the token whose keys to list.
    ///
    /// If omitted, the single available token is used.
    pub uri: Option<String>,
}

impl ListKeysArgs {
    pub fn build_command(self, config: &TEdgeConfig) -> Result<Box<dyn Command>, ConfigError> {
        let cryptoki_config = config
            .device
            .cryptoki_config(None::<&dyn CloudConfig>)?
            .context("Cryptoki config is not enabled")?;

        Ok(ListKeysCmd {
            cryptoki_config,
            uri: self.uri,
            pin: self.pin,
        }
        .into_boxed())
    }
}

pub struct ListKeysCmd {
    pub cryptoki_config: CryptokiConfig,
    pub uri: Option<String>,
    pub pin: Option<String>,
}

#[async_trait::async_trait]
impl Command for ListKeysCmd {
    fn description(&self) -> String {
        "List the keys on a PKCS #11 token.".into()
    }

    async fn execute(&self, _config: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        let cryptoki = tedge_p11::tedge_p11_service(self.cryptoki_config.clone())?;
        let response = cryptoki.list_keys(ListKeysRequest {
            uri: self.uri.clone(),
            pin: self.pin.clone().map(SecretString::from),
        })?;

        if response.keys.is_empty() {
            eprintln!("No keys were found on the token.");
            return Ok(());
        }

        for (i, key) in response.keys.iter().enumerate() {
            if i > 0 {
                println!();
            }
            let title = if key.class == "private" {
                "Private key"
            } else {
                "Public key"
            };
            println!("{title}");
            if !key.label.is_empty() {
                println!("  Label: {}", key.label);
            }
            if !key.id.is_empty() {
                println!("  Id:    {}", key.id);
            }
            if !key.key_type.is_empty() {
                println!("  Type:  {}", key.key_type);
            }
            println!("  URI:   {}", key.uri);
        }

        Ok(())
    }
}
