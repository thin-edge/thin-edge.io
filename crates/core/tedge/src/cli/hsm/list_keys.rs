use anyhow::Context;
use tedge_config::tedge_toml::CloudConfig;
use tedge_config::TEdgeConfig;
use tedge_p11::pkcs11::uri::Pkcs11Uri;
use tedge_p11::service::ListKeysRequest;
use tedge_p11::CryptokiConfig;
use tedge_p11::SecretString;

use super::create_key::parse_id;
use crate::command::Command;
use crate::log::MaybeFancy;
use crate::ConfigError;

/// Arguments of the key-listing command.
#[derive(Debug, clap::Args)]
pub struct ListKeysArgs {
    /// Only list keys with this label (CKA_LABEL).
    #[arg(long)]
    pub label: Option<String>,

    /// Only list keys with this id (CKA_ID), as hex digits without `0x` prefix, e.g. `--id 010203`.
    #[arg(long)]
    pub id: Option<String>,

    /// User PIN for logging into the token (required to list private keys).
    ///
    /// If omitted, the PIN configured for tedge-p11-server is used.
    #[arg(long)]
    pub pin: Option<String>,

    /// A PKCS #11 URI selecting the token whose keys to list.
    ///
    /// If omitted, the single available token is used. The URI may also carry `object=` and `id=`
    /// attributes to only list matching keys, equivalent to the --label and --id flags.
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
            label: self.label,
            id: self.id,
            pin: self.pin,
        }
        .into_boxed())
    }
}

pub struct ListKeysCmd {
    pub cryptoki_config: CryptokiConfig,
    pub uri: Option<String>,
    pub label: Option<String>,
    pub id: Option<String>,
    pub pin: Option<String>,
}

#[async_trait::async_trait]
impl Command for ListKeysCmd {
    fn description(&self) -> String {
        "List the keys on a PKCS #11 token.".into()
    }

    async fn execute(&self, _config: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        // Filters can come from the flags or from the `object=`/`id=` attributes of the URI;
        // specifying the same filter both ways is ambiguous, so it is rejected.
        let uri_attributes = self
            .uri
            .as_deref()
            .map(|uri| Pkcs11Uri::parse(uri).context("Failed to parse PKCS #11 URI"))
            .transpose()?;
        let (uri_object, uri_id) = uri_attributes
            .map(|uri| (uri.object.map(|o| o.into_owned()), uri.id))
            .unwrap_or_default();

        if self.label.is_some() && uri_object.is_some() {
            return Err(anyhow::anyhow!(
                "The key label was given both with --label and the `object` URI attribute; use only one."
            )
            .into());
        }
        if self.id.is_some() && uri_id.is_some() {
            return Err(anyhow::anyhow!(
                "The key id was given both with --id and the `id` URI attribute; use only one."
            )
            .into());
        }

        let label_filter = self.label.clone().or(uri_object);
        // Key ids are compared in the hex format used by the listing (lowercase, no separators).
        let id_filter = self
            .id
            .as_ref()
            .map(|s| parse_id(s))
            .transpose()
            .context("invalid id")?
            .or(uri_id)
            .map(|id| id.iter().map(|b| format!("{b:02x}")).collect::<String>());

        let cryptoki = tedge_p11::tedge_p11_service(self.cryptoki_config.clone())?;
        let response = cryptoki.list_keys(ListKeysRequest {
            uri: self.uri.clone(),
            pin: self.pin.clone().map(SecretString::from),
        })?;

        // Filtering is done here rather than by the PKCS #11 service, so it also works when
        // talking to an older tedge-p11-server.
        let keys: Vec<_> = response
            .keys
            .iter()
            .filter(|key| {
                label_filter
                    .as_deref()
                    .is_none_or(|label| key.label == label)
            })
            .filter(|key| id_filter.as_deref().is_none_or(|id| key.id == id))
            .collect();

        if keys.is_empty() {
            if label_filter.is_some() || id_filter.is_some() {
                eprintln!("No keys matching the given filters were found on the token.");
            } else {
                eprintln!("No keys were found on the token.");
            }
            return Ok(());
        }

        for (i, key) in keys.iter().enumerate() {
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
