use anyhow::Context;
use tedge_config::tedge_toml::CloudConfig;
use tedge_config::TEdgeConfig;
use tedge_p11::service::InitTokenRequest;
use tedge_p11::CryptokiConfig;
use tedge_p11::SecretString;

use crate::cli::common::Cloud;
use crate::cli::common::CloudArg;
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

    /// The slot id to initialize.
    ///
    /// If not provided, the single slot holding an uninitialized token is selected
    /// automatically.
    #[arg(long)]
    pub slot: Option<u64>,

    #[clap(subcommand)]
    pub cloud: Option<CloudArg>,
}

impl InitArgs {
    pub fn build_command(self, config: &TEdgeConfig) -> Result<Box<dyn Command>, ConfigError> {
        let cloud: Option<Cloud> = self.cloud.map(<_>::try_into).transpose()?;
        let cloud_config = match cloud.as_ref() {
            Some(c) => Some(config.as_cloud_config(c.into())?),
            None => None,
        };
        let cryptoki_config = config
            .device
            .cryptoki_config(cloud_config.as_ref().map(|c| &**c as &dyn CloudConfig))?
            .context("Cryptoki config is not enabled")?;

        Ok(InitTokenHsmCmd {
            cryptoki_config,
            label: self.label,
            so_pin: self.so_pin,
            pin: self.pin,
            slot: self.slot,
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
    pub slot: Option<u64>,
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
            slot: self.slot,
        })?;

        // Human-readable status goes to stderr, so that stdout carries only the token URI and the
        // command can be used in scripts, e.g. `URI=$(tedge hsm init)`.
        eprintln!("Token '{}' is initialized and ready to use.", self.label);
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
