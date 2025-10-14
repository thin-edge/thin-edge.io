use clap::ValueEnum;
use tedge_config::TEdgeConfig;
use tedge_p11_server::pkcs11::CreateKeyParams;
use tedge_p11_server::pkcs11::KeyTypeParams;
use tedge_p11_server::CryptokiConfig;

use crate::cli::common::Cloud;
use crate::command::Command;
use crate::log::MaybeFancy;

pub struct CreateKeyCmd {
    pub cryptoki_config: CryptokiConfig,
    pub bits: RsaBits,
    pub curve: EcCurve,
    pub label: String,
    pub r#type: KeyType,
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
impl Command for CreateKeyCmd {
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

        let cryptoki = tedge_p11_server::tedge_p11_service(self.cryptoki_config.clone())?;
        let Some(token) = self.token.clone() else {
            eprintln!("No token URL was provided for this operation; the available tokens are:");
            let tokens = cryptoki.get_tokens_uris()?;
            for token_uri in tokens {
                eprintln!("{token_uri}");
            }
            return Ok(());
        };

        let params = CreateKeyParams {
            key,
            label: self.label.clone(),
        };

        // generate a keypair
        // should probably verify the keys before using them
        let key = cryptoki.create_key(&token, params)?;
        let pubkey_pem = key.pem;
        let uri = key.uri;

        eprintln!("New keypair was successfully created.");
        eprintln!("Key URI: {uri}");
        eprintln!("Public key:\n{pubkey_pem}\n");

        save_key_uri_to_config(config, self.cloud.as_ref(), &uri).await?;

        Ok(())
    }
}

async fn save_key_uri_to_config(
    config: TEdgeConfig,
    cloud: Option<&Cloud>,
    uri: &str,
) -> anyhow::Result<()> {
    config
        .update_toml(&|dto, _reader| {
            // XXX: can break if the keys ever change and having to use strings sucks
            let profile = cloud.as_ref().and_then(|c| c.profile_name());
            // XXX: this should be elsewhere!
            let cloud = cloud.map(|c| match c {
                crate::cli::common::MaybeBorrowedCloud::Aws(_) => "aws",
                crate::cli::common::MaybeBorrowedCloud::Azure(_) => "az",
                crate::cli::common::MaybeBorrowedCloud::C8y(_) => "c8y",
            });
            let mut key = "device.key_uri".to_string();

            if let Some(profile) = profile {
                key = format!("profiles.{profile}.{key}");
            }

            if let Some(cloud) = cloud {
                key = format!("{cloud}.{key}");
            }

            let key = key.parse().expect("should be valid WritableKeys");
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
}
