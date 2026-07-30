use anyhow::Context;
use tedge_config::tedge_toml::CloudConfig;
use tedge_config::TEdgeConfig;
use tedge_p11::service::InitTokenRequest;
use tedge_p11::CryptokiConfig;
use tedge_p11::SecretString;

use crate::command::Command;
use crate::log::MaybeFancy;
use crate::ConfigError;

/// Arguments of the token-initialization command.
#[derive(Debug, clap::Args)]
pub struct InitArgs {
    /// Label (CKA_LABEL) to assign to the initialized token.
    #[arg(long, default_value = "tedge")]
    pub label: String,

    /// Security Officer (SO) PIN used to initialize the token.
    ///
    /// If not provided, the user PIN is used as the SO PIN, which works for tokens that do not
    /// enforce distinct PINs (e.g. SoftHSM2).
    #[arg(long)]
    pub so_pin: Option<String>,

    /// User PIN to set on the token.
    ///
    /// If not provided, the PIN configured for tedge-p11-server is used.
    #[arg(long)]
    pub pin: Option<String>,

    /// A PKCS #11 URI selecting the slot to initialize, e.g. `pkcs11:slot-id=1`.
    ///
    /// If not provided and only 1 uninitialized slot is found, then it is used. If more than 1 is
    /// found, then an explicit slot must be given. Use `tedge hsm list-tokens` to see the URI of
    /// each slot.
    pub uri: Option<String>,
}

impl InitArgs {
    pub fn build_command(self, config: &TEdgeConfig) -> Result<Box<dyn Command>, ConfigError> {
        // Initializing a token is not scoped to a cloud; the slot is selected by URI and nothing
        // cloud-specific is written to the configuration.
        let cryptoki_config = config
            .device
            .cryptoki_config(None::<&dyn CloudConfig>)?
            .context("Cryptoki config is not enabled")?;

        Ok(InitTokenHsmCmd {
            cryptoki_config,
            label: self.label,
            so_pin: self.so_pin,
            pin: self.pin,
            uri: self.uri,
        }
        .into_boxed())
    }
}

/// Initialize a PKCS #11 token so that it can be used to store keys.
pub struct InitTokenHsmCmd {
    pub cryptoki_config: CryptokiConfig,
    pub label: String,
    pub so_pin: Option<String>,
    pub pin: Option<String>,
    pub uri: Option<String>,
}

#[async_trait::async_trait]
impl Command for InitTokenHsmCmd {
    fn description(&self) -> String {
        "Initialize a PKCS #11 token.".into()
    }

    async fn execute(&self, _config: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        let cryptoki = tedge_p11::tedge_p11_service(self.cryptoki_config.clone())?;

        let response = cryptoki.init_token(InitTokenRequest {
            label: self.label.clone(),
            so_pin: self.so_pin.clone().map(SecretString::from),
            pin: self.pin.clone().map(SecretString::from),
            uri: self.uri.clone(),
        })?;

        // Human-readable status goes to stderr, so that stdout carries only the token URI and the
        // command can be used in scripts, e.g. `URI=$(tedge hsm init)`. Report the resulting token
        // by its URI rather than the requested --label, since an existing token may have been
        // reused (e.g. a pre-initialized Nitrokey whose label differs from --label).
        eprintln!("A PKCS #11 token is initialized and ready to use.");
        eprintln!(
            "You can now create a keypair on it, e.g. `tedge hsm create-key \"{}\"`.",
            response.uri
        );

        // The token URI is the machine-readable output of this command: print it (and nothing else)
        // to stdout.
        println!("{}", response.uri);

        Ok(())
    }
}
