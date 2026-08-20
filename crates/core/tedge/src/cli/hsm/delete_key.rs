use std::io::Write;

use anyhow::Context;
use tedge_config::tedge_toml::CloudConfig;
use tedge_config::TEdgeConfig;
use tedge_p11::service::DeleteKeyRequest;
use tedge_p11::CryptokiConfig;
use tedge_p11::SecretString;

use super::create_key::encode_id_attr;
use super::create_key::encode_uri_attr;
use super::create_key::parse_id;
use crate::command::Command;
use crate::log::MaybeFancy;
use crate::ConfigError;

/// Arguments of the key-deletion command.
#[derive(Debug, clap::Args)]
pub struct DeleteKeyArgs {
    /// Label (CKA_LABEL) of the key to delete.
    #[arg(long)]
    pub label: Option<String>,

    /// Id (CKA_ID) of the key to delete, as hex digits without `0x` prefix, e.g. `--id 010203`.
    #[arg(long)]
    pub id: Option<String>,

    /// User PIN for logging into the token. If omitted, the configured PIN is used.
    #[arg(long)]
    pub pin: Option<String>,

    /// Delete without prompting for confirmation, and bypass the active-key safety check.
    #[arg(long, default_value_t = false)]
    pub force: bool,

    /// The URI of the token holding the key.
    ///
    /// If omitted, the configured/single token is used. A full key URI (with `object=`/`id=`) may
    /// be given here instead of using --label/--id.
    pub token: Option<String>,
}

impl DeleteKeyArgs {
    pub fn build_command(self, config: &TEdgeConfig) -> Result<Box<dyn Command>, ConfigError> {
        let cryptoki_config = config
            .device
            .cryptoki_config(None::<&dyn CloudConfig>)?
            .context("Cryptoki config is not enabled")?;

        Ok(DeleteKeyCmd {
            cryptoki_config,
            label: self.label,
            id: self.id,
            pin: self.pin,
            force: self.force,
            token: self.token,
        }
        .into_boxed())
    }
}

pub struct DeleteKeyCmd {
    pub cryptoki_config: CryptokiConfig,
    pub label: Option<String>,
    pub id: Option<String>,
    pub pin: Option<String>,
    pub force: bool,
    pub token: Option<String>,
}

#[async_trait::async_trait]
impl Command for DeleteKeyCmd {
    fn description(&self) -> String {
        "Delete a key from a PKCS #11 token.".into()
    }

    async fn execute(&self, config: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        let id = self
            .id
            .as_ref()
            .map(|s| parse_id(s))
            .transpose()
            .context("invalid id")?;

        let uri = build_selector_uri(self.token.as_deref(), self.label.as_deref(), id.as_deref());

        // Guard against deleting nothing-in-particular: the selector must pin down an object.
        if !(uri.contains("object=") || uri.contains("id=")) {
            return Err(MaybeFancy::Unfancy(anyhow::anyhow!(
                "Specify the key to delete with --label and/or --id, or pass a URI that selects an \
                 object."
            )));
        }

        // Guard against deleting the key the device is currently configured to use for signing.
        if !self.force {
            if let Some((setting, key_uri)) =
                active_key_match(&config, self.label.as_deref(), id.as_deref())
            {
                return Err(MaybeFancy::Unfancy(anyhow::anyhow!(
                    "Refusing to delete: this key is referenced by `{setting}` ({key_uri}), which \
                     the device uses to connect to the cloud. Deleting it would break the \
                     connection. Re-run with --force if you are sure.",
                )));
            }
        }

        // Confirm before the destructive action unless --force was given.
        if !self.force
            && !confirm(&format!(
                "This will permanently delete key object(s) matching '{uri}'."
            ))?
        {
            eprintln!("Aborted; no keys were deleted.");
            return Ok(());
        }

        let cryptoki = tedge_p11::tedge_p11_service(self.cryptoki_config.clone())?;
        let response = cryptoki.delete_key(DeleteKeyRequest {
            uri,
            pin: self.pin.clone().map(SecretString::from),
        })?;

        // Deleting a key that does not exist is not an error: the command is idempotent, so this
        // exits successfully with a note rather than failing.
        if response.deleted.is_empty() {
            eprintln!("No matching key was found; nothing to delete.");
        } else {
            eprintln!("Deleted {} object(s):", response.deleted.len());
            for uri in &response.deleted {
                eprintln!("  {uri}");
            }
        }

        Ok(())
    }
}

/// Builds a PKCS #11 URI that selects the object(s) to delete from `token` (or any token) by label
/// and/or id.
fn build_selector_uri(token: Option<&str>, label: Option<&str>, id: Option<&[u8]>) -> String {
    let mut uri = token.unwrap_or("pkcs11:").to_string();
    let push = |uri: &mut String, attr: &str| {
        if uri != "pkcs11:" && !uri.ends_with(';') {
            uri.push(';');
        }
        uri.push_str(attr);
    };
    if let Some(label) = label {
        push(&mut uri, &format!("object={}", encode_uri_attr(label)));
    }
    if let Some(id) = id {
        push(&mut uri, &format!("id={}", encode_id_attr(id)));
    }
    uri
}

/// Returns the config setting and URI of a configured signing key that the given selector would
/// match, so we can refuse to delete the active key. Only checks the common device/cloud key URIs.
fn active_key_match(
    config: &TEdgeConfig,
    label: Option<&str>,
    id: Option<&[u8]>,
) -> Option<(String, String)> {
    // Nothing to compare against if no label/id was given (e.g. a raw token URI was passed).
    if label.is_none() && id.is_none() {
        return None;
    }
    let object_frag = label.map(|l| format!("object={}", encode_uri_attr(l)));
    let id_frag = id.map(|id| format!("id={}", encode_id_attr(id)));

    let settings = [
        "device.key_uri",
        "c8y.device.key_uri",
        "az.device.key_uri",
        "aws.device.key_uri",
    ];
    for setting in settings {
        let Ok(key) = setting.parse() else { continue };
        let Ok(uri) = config.read_string(&key) else {
            continue;
        };
        if uri.is_empty() {
            continue;
        }
        if uri_matches(&uri, object_frag.as_deref(), id_frag.as_deref()) {
            return Some((setting.to_string(), uri));
        }
    }
    None
}

/// Returns true if `configured` (a PKCS #11 URI) contains the given `object=`/`id=` attributes.
/// Attributes are matched as whole `;`-separated components so `object=key` does not match
/// `object=key2`.
fn uri_matches(configured: &str, object_frag: Option<&str>, id_frag: Option<&str>) -> bool {
    let components: Vec<&str> = configured
        .trim_start_matches("pkcs11:")
        .split(';')
        .collect();
    let object_ok = object_frag.map(|f| components.contains(&f)).unwrap_or(true);
    let id_ok = id_frag.map(|f| components.contains(&f)).unwrap_or(true);
    object_ok && id_ok
}

/// Prompts the user for a yes/no confirmation on stderr, defaulting to no.
fn confirm(message: &str) -> anyhow::Result<bool> {
    eprint!("{message} Continue? [y/N]: ");
    std::io::stderr().flush().ok();
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .context("Failed to read confirmation")?;
    Ok(matches!(line.trim().to_lowercase().as_str(), "y" | "yes"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_selector_uri_from_token_label_and_id() {
        assert_eq!(
            build_selector_uri(
                Some("pkcs11:token=tedge"),
                Some("my-key"),
                Some(&[0x01, 0xab])
            ),
            "pkcs11:token=tedge;object=my-key;id=%01%AB"
        );
    }

    #[test]
    fn builds_selector_uri_without_token() {
        assert_eq!(
            build_selector_uri(None, Some("my-key"), None),
            "pkcs11:object=my-key"
        );
        assert_eq!(
            build_selector_uri(None, None, Some(&[0x02])),
            "pkcs11:id=%02"
        );
    }

    #[test]
    fn selector_uri_encodes_reserved_characters_in_label() {
        assert_eq!(
            build_selector_uri(None, Some("a b;c"), None),
            "pkcs11:object=a%20b%3Bc"
        );
    }

    #[test]
    fn uri_matches_whole_components_only() {
        let configured = "pkcs11:token=tedge;object=my-key;id=%01";
        // exact label matches
        assert!(uri_matches(configured, Some("object=my-key"), None));
        // a prefix of the label must not match
        assert!(!uri_matches(configured, Some("object=my"), None));
        // label + id both must match
        assert!(uri_matches(
            configured,
            Some("object=my-key"),
            Some("id=%01")
        ));
        assert!(!uri_matches(
            configured,
            Some("object=my-key"),
            Some("id=%02")
        ));
        // id-only match
        assert!(uri_matches(configured, None, Some("id=%01")));
    }
}
