use anyhow::Context;
use camino::Utf8Path;
use clap::ValueEnum;
use tedge_config::tedge_toml::WritableKey;
use tedge_config::TEdgeConfig;
use tedge_p11_server::pkcs11::CreateKeyParams;
use tedge_p11_server::pkcs11::KeyTypeParams;
use tedge_p11_server::service::CreateKeyRequest;
use tedge_p11_server::CryptokiConfig;
use tedge_p11_server::SecretString;
use tracing::warn;

use crate::cli::common::Cloud;
use crate::command::Command;
use crate::log::MaybeFancy;

pub struct CreateKeyHsmCmd {
    pub cryptoki_config: CryptokiConfig,
    pub bits: RsaBits,
    pub curve: EcCurve,
    pub label: String,
    pub r#type: KeyType,
    pub id: Option<String>,
    pub pin: Option<String>,
    pub outfile_pubkey: Option<Box<Utf8Path>>,
    pub cloud: Option<Cloud>,
    pub token: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum KeyType {
    Rsa,
    Ecdsa,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum RsaBits {
    #[value(name = "2048")]
    Bits2048,
    #[value(name = "3072")]
    Bits3072,
    #[value(name = "4096")]
    Bits4096,
}

impl From<RsaBits> for u16 {
    fn from(value: RsaBits) -> Self {
        match value {
            RsaBits::Bits2048 => 2048,
            RsaBits::Bits3072 => 3072,
            RsaBits::Bits4096 => 4096,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum EcCurve {
    P256,
    P384,
}

impl From<EcCurve> for u16 {
    fn from(value: EcCurve) -> Self {
        match value {
            EcCurve::P256 => 256,
            EcCurve::P384 => 384,
        }
    }
}

#[async_trait::async_trait]
impl Command for CreateKeyHsmCmd {
    fn description(&self) -> String {
        "Generate a keypair.".into()
    }

    async fn execute(&self, config: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        let key = match self.r#type {
            KeyType::Rsa => KeyTypeParams::Rsa {
                bits: self.bits.into(),
            },
            KeyType::Ecdsa => KeyTypeParams::Ec {
                curve: self.curve.into(),
            },
        };
        let id = self
            .id
            .as_ref()
            .map(|s| parse_id(s))
            .transpose()
            .context("invalid id")?;

        let cryptoki = tedge_p11_server::tedge_p11_service(self.cryptoki_config.clone())?;
        let Some(token) = self.token.clone() else {
            eprintln!("No token URL was provided for this operation; the available tokens are:");
            let tokens = cryptoki.get_tokens_uris()?;
            for token_uri in tokens {
                eprintln!("{token_uri}");
            }
            std::process::exit(1);
        };

        let params = CreateKeyParams {
            key,
            label: self.label.clone(),
            id,
        };

        // generate a keypair
        // should probably verify the keys before using them
        let key = cryptoki.create_key(CreateKeyRequest {
            uri: token,
            params,
            pin: self.pin.clone().map(SecretString::from),
        })?;

        let pubkey_pem = key.pem;
        let uri = key.uri;
        eprintln!("New keypair was successfully created.");
        eprintln!("Key URI: {uri}");
        eprintln!("Public key:\n{pubkey_pem}\n");

        // Operations below may fail for some reason (e.g. no permissions to write to outfile), but
        // the key was still created, so we still consider the operation succeeded.
        if let Some(outfile) = &self.outfile_pubkey {
            let r = std::fs::write(outfile.as_ref(), pubkey_pem);
            if let Err(e) = r {
                warn!(?e, path=%outfile, "Failed to save the public key to file");
            }
        }

        if let Err(e) = save_key_uri_to_config(config, self.cloud.as_ref(), &uri).await {
            warn!(?e, "Failed to save public key URI to tedge-config. You may need to enter key URI in tedge-config manually to use the new key.")
        }

        Ok(())
    }
}

async fn save_key_uri_to_config(
    config: TEdgeConfig,
    cloud: Option<&Cloud>,
    uri: &str,
) -> anyhow::Result<()> {
    let key = extract_device_id_for_cloud(cloud)?;
    config
        .update_toml(&|dto, _reader| {
            let r = dto.try_update_str(&key, uri).map_err(|e| e.into());
            eprintln!(
                "The `{key}` configuration setting was updated with the newly created key's URI"
            );
            r
        })
        .await
        .map_err(anyhow::Error::new)?;

    Ok(())
}

/// Given a cloud (and possibly profile) return the correct `device.key_uri` key under the correct table.
///
/// - `device.key_uri` if cloud is `None`
/// - `{cloud_name}.device.key_uri` if cloud is `Some(Cloud{profile: None})`
/// - `{cloud_name}.profiles.{profile}.device.key_uri` if cloud is `Some(Cloud{profile: Some("profile")})`
fn extract_device_id_for_cloud(
    cloud: Option<&crate::cli::common::MaybeBorrowedCloud<'static>>,
) -> anyhow::Result<tedge_config::tedge_toml::WritableKey> {
    // XXX: can break if the keys ever change and having to use strings sucks

    // Additionally, these kind of config transformation should probably live somewhere in tedge-config, but it's
    // currently (#3835) being rewritten to support generic mapper configurations which will make using per-profile
    // settings (which device.key_uri is) much simpler and so the cleaner implementation of this would have to be
    // updated anyway. As such, it's left as is and when generic mapping configurations are merged, it will have to be
    // revised.
    let mut key = "device.key_uri".to_string();
    let profile = cloud.as_ref().and_then(|c| c.profile_name());
    if let Some(profile) = profile {
        key = format!("profiles.{profile}.{key}");
    }

    let cloud = cloud.map(|c| match c {
        crate::cli::common::MaybeBorrowedCloud::Aws(_) => "aws",
        crate::cli::common::MaybeBorrowedCloud::Azure(_) => "az",
        crate::cli::common::MaybeBorrowedCloud::C8y(_) => "c8y",
    });

    if let Some(cloud) = cloud {
        key = format!("{cloud}.{key}");
    }

    key.parse::<WritableKey>()
        .with_context(|| format!("failed to parse '{key}' as a WritableKey"))
}

/// Parses id provided as a sequence of bytes encoded as pair of hex digits without `0x` prefix, optionally separated by
/// spaces.
fn parse_id(id_hexstr: &str) -> anyhow::Result<Vec<u8>> {
    let id_hexstr = id_hexstr.trim();

    anyhow::ensure!(!id_hexstr.is_empty(), "ID must not be empty");

    let mut bytes = Vec::new();
    let mut chars = id_hexstr.char_indices();
    while let Some((i1, c1)) = chars.next() {
        if c1.is_whitespace() {
            continue;
        }

        anyhow::ensure!(c1.is_ascii_hexdigit(), "{c1} is not a hex digit");

        let Some((i2, c2)) = chars.next() else {
            anyhow::bail!("expected hex digit after {c1}");
        };
        anyhow::ensure!(c2.is_ascii_hexdigit(), "expected hex digit after {c1}");

        let num = u8::from_str_radix(&id_hexstr[i1..=i2], 16)
            .with_context(|| format!("failed to parse {} as u8", &id_hexstr[i1..=i2]))?;
        bytes.push(num);
    }

    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use tedge_test_utils::fs::TempTedgeDir;

    use crate::cli::common::MaybeBorrowedCloud;

    use super::*;

    #[tokio::test]
    async fn saves_uri_under_correct_key() {
        let tempdir = TempTedgeDir::new();

        assert_saves_under_key(None, "device.key_uri", &tempdir).await;

        assert_saves_under_key(
            Some(&MaybeBorrowedCloud::c8y(None)),
            "c8y.device.key_uri",
            &tempdir,
        )
        .await;

        assert_saves_under_key(
            Some(&MaybeBorrowedCloud::c8y(Some(
                "profile1".to_string().try_into().unwrap(),
            ))),
            "c8y.profiles.profile1.device.key_uri",
            &tempdir,
        )
        .await;

        assert_saves_under_key(
            Some(&MaybeBorrowedCloud::az(None)),
            "az.device.key_uri",
            &tempdir,
        )
        .await;

        assert_saves_under_key(
            Some(&MaybeBorrowedCloud::az(Some(
                "profile1".to_string().try_into().unwrap(),
            ))),
            "az.profiles.profile1.device.key_uri",
            &tempdir,
        )
        .await;

        assert_saves_under_key(
            Some(&MaybeBorrowedCloud::aws(None)),
            "aws.device.key_uri",
            &tempdir,
        )
        .await;

        assert_saves_under_key(
            Some(&MaybeBorrowedCloud::aws(Some(
                "profile1".to_string().try_into().unwrap(),
            ))),
            "aws.profiles.profile1.device.key_uri",
            &tempdir,
        )
        .await;
    }

    async fn assert_saves_under_key(cloud: Option<&Cloud>, key: &str, tempdir: &TempTedgeDir) {
        let config = TEdgeConfig::load(tempdir.path()).await.unwrap();
        save_key_uri_to_config(config, cloud, "pkcs11:hello")
            .await
            .unwrap();

        let config = TEdgeConfig::load(tempdir.path()).await.unwrap();
        assert_eq!(
            config.read_string(&key.parse().unwrap()).unwrap(),
            "pkcs11:hello"
        );

        std::fs::remove_file(tempdir.file("tedge.toml").path()).unwrap();
    }

    #[test]
    fn parses_id() {
        assert_eq!(parse_id("01 02 03").unwrap(), vec![0x01, 0x02, 0x03]);
        assert_eq!(parse_id("010203").unwrap(), vec![0x01, 0x02, 0x03]);
        assert_eq!(parse_id("0102 03").unwrap(), vec![0x01, 0x02, 0x03]);

        assert_eq!(
            parse_id("    ").unwrap_err().to_string(),
            "ID must not be empty"
        );

        assert_eq!(
            parse_id("0").unwrap_err().to_string(),
            "expected hex digit after 0"
        );

        assert_eq!(
            parse_id("  0   1   2   3 ").unwrap_err().to_string(),
            "expected hex digit after 0"
        );
    }
}
