use anyhow::Context;
use tedge_config::tedge_toml::CloudConfig;
use tedge_config::tedge_toml::WritableKey;
use tedge_config::TEdgeConfig;
use tedge_p11::service::ChangePinRequest;
use tedge_p11::CryptokiConfig;
use tedge_p11::SecretString;

use crate::command::Command;
use crate::log::MaybeFancy;
use crate::ConfigError;

/// Arguments of the PIN-change command.
#[derive(Debug, clap::Args)]
pub struct ChangePinArgs {
    /// The new user PIN to set. If omitted, it is prompted for interactively (with confirmation).
    #[arg(long)]
    pub new_pin: Option<String>,

    /// The current user PIN.
    ///
    /// If omitted, the PIN configured for tedge-p11-server is used. Ignored when `--reset` is
    /// given.
    #[arg(long)]
    pub current_pin: Option<String>,

    /// Reset the user PIN using the Security Officer PIN instead of the current user PIN.
    ///
    /// Use this to recover a token when the current user PIN is unknown or the token is locked
    /// out. Requires the Security Officer PIN.
    #[arg(long, default_value_t = false)]
    pub reset: bool,

    /// The Security Officer PIN, used with `--reset`. If omitted, it is prompted for interactively.
    #[arg(long)]
    pub so_pin: Option<String>,

    /// A PKCS #11 URI selecting the token whose user PIN to change.
    ///
    /// If omitted, the single initialized token is selected automatically. If several initialized
    /// tokens exist, the command fails and lists their URIs so one can be selected.
    pub uri: Option<String>,
}

impl ChangePinArgs {
    pub fn build_command(self, config: &TEdgeConfig) -> Result<Box<dyn Command>, ConfigError> {
        // Changing a token PIN is not scoped to a cloud; the token is selected by URI.
        let cryptoki_config = config
            .device
            .cryptoki_config(None::<&dyn CloudConfig>)?
            .context("Cryptoki config is not enabled")?;

        Ok(ChangePinCmd {
            cryptoki_config,
            uri: self.uri,
            new_pin: self.new_pin,
            current_pin: self.current_pin,
            reset: self.reset,
            so_pin: self.so_pin,
        }
        .into_boxed())
    }
}

pub struct ChangePinCmd {
    pub cryptoki_config: CryptokiConfig,
    pub uri: Option<String>,
    pub new_pin: Option<String>,
    pub current_pin: Option<String>,
    pub reset: bool,
    pub so_pin: Option<String>,
}

#[async_trait::async_trait]
impl Command for ChangePinCmd {
    fn description(&self) -> String {
        "Change the user PIN of a PKCS #11 token.".into()
    }

    async fn execute(&self, config: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        // Resolve the new PIN, prompting (with confirmation) when it wasn't passed as a flag so it
        // doesn't end up in the shell history or process list.
        let new_pin = match self.new_pin.clone() {
            Some(pin) => pin,
            None => prompt_new_pin()?,
        };

        // For a reset we need the Security Officer PIN; prompt for it if not provided.
        let so_pin = if self.reset {
            let so_pin = match self.so_pin.clone() {
                Some(pin) => pin,
                None => rpassword::prompt_password("Enter Security Officer PIN: ")
                    .context("Failed to read the Security Officer PIN")?,
            };
            Some(SecretString::from(so_pin))
        } else {
            self.so_pin.clone().map(SecretString::from)
        };

        let cryptoki = tedge_p11::tedge_p11_service(self.cryptoki_config.clone())?;
        let response = cryptoki.change_pin(ChangePinRequest {
            uri: self.uri.clone(),
            new_pin: SecretString::from(new_pin.clone()),
            old_pin: self.current_pin.clone().map(SecretString::from),
            so_pin,
            reset: self.reset,
        })?;

        if self.reset {
            eprintln!("The user PIN of token '{}' was reset.", response.uri);
        } else {
            eprintln!("The user PIN of token '{}' was changed.", response.uri);
        }

        // Keep tedge-p11-server's configured PIN in sync so signing keeps working with the new PIN.
        // Without this the provider would keep using the old PIN and fail to log in to the token.
        match save_pin_to_config(config, &new_pin).await {
            Ok(()) => {
                eprintln!(
                    "The `device.cryptoki.pin` configuration setting was updated with the new PIN."
                );
                eprintln!(
                    "Restart tedge-p11-server for the change to take effect, e.g. \
                     `tedgectl restart tedge-p11-server` (or your service manager's equivalent)."
                );
            }
            Err(e) => {
                eprintln!(
                    "Warning: the token PIN was changed but `device.cryptoki.pin` could not be \
                     updated ({e:#}). Update it manually with \
                     `tedge config set device.cryptoki.pin <new-pin>` and restart tedge-p11-server, \
                     otherwise signing will fail."
                );
            }
        }

        Ok(())
    }
}

/// Prompts for a new PIN twice and returns it once both entries match.
fn prompt_new_pin() -> anyhow::Result<String> {
    let pin =
        rpassword::prompt_password("Enter new user PIN: ").context("Failed to read the new PIN")?;
    let confirm = rpassword::prompt_password("Confirm new user PIN: ")
        .context("Failed to read the new PIN confirmation")?;
    anyhow::ensure!(pin == confirm, "The entered PINs do not match.");
    anyhow::ensure!(!pin.is_empty(), "The new PIN must not be empty.");
    Ok(pin)
}

/// Updates `device.cryptoki.pin` in tedge config so the PKCS #11 provider uses the new PIN.
async fn save_pin_to_config(config: TEdgeConfig, new_pin: &str) -> anyhow::Result<()> {
    let key = "device.cryptoki.pin"
        .parse::<WritableKey>()
        .context("failed to parse 'device.cryptoki.pin' as a WritableKey")?;
    config
        .update_toml(&|dto, _reader| dto.try_update_str(&key, new_pin).map_err(|e| e.into()))
        .await
        .map_err(anyhow::Error::new)?;

    Ok(())
}
