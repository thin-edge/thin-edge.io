use camino::Utf8PathBuf;
use certificate::CsrTemplate;
use clap::ValueEnum;
use tedge_config::TEdgeConfig;
use tedge_p11_server::pkcs11::CreateKeyParams;
use tedge_p11_server::pkcs11::KeyTypeParams;
use tedge_p11_server::CryptokiConfig;

use crate::command::Command;
use crate::log::MaybeFancy;

pub struct CreateKeyCmd {
    pub cryptoki_config: CryptokiConfig,
    pub bits: u16,
    pub curve: u16,
    pub label: String,
    pub r#type: KeyType,
    /// The device identifier to be used as the common name for the certificate
    pub device_id: String,
    pub csr_template: CsrTemplate,
    pub csr_path: Utf8PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum KeyType {
    Rsa,
    Ec,
}

#[async_trait::async_trait]
impl Command for CreateKeyCmd {
    fn description(&self) -> String {
        "Generate a keypair.".into()
    }

    async fn execute(&self, _config: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        let key = match self.r#type {
            KeyType::Rsa => KeyTypeParams::Rsa { bits: self.bits },
            KeyType::Ec => KeyTypeParams::Ec { curve: self.curve },
        };
        let params = CreateKeyParams {
            key,
            token: None,
            label: self.label.clone(),
        };

        // generate a keypair
        // should probably verify the keys before using them
        let cryptoki = tedge_p11_server::tedge_p11_service(self.cryptoki_config.clone())?;
        let pubkey_pem = cryptoki.create_key(None, params)?;

        eprintln!("New keypair was successfully created.");

        // use returned public key to create a CSR
        let sigalg = match (self.r#type, self.curve) {
            (KeyType::Rsa, _) => certificate::SignatureAlgorithm::RsaPkcs1Sha256,
            (KeyType::Ec, 256) => certificate::SignatureAlgorithm::EcdsaP256Sha256,
            (KeyType::Ec, 384) => certificate::SignatureAlgorithm::EcdsaP384Sha384,
            _ => {
                return Err(
                    anyhow::anyhow!("invalid arguments: bad keytype/arg combination").into(),
                )
            }
        };

        let key = super::create_csr::Key::Cryptoki {
            config: self.cryptoki_config.clone(),
            privkey_label: Some(self.label.clone()),
            pubkey_pem: Some(pubkey_pem.clone()),
            sigalg: Some(sigalg),
        };

        super::create_device_csr(
            self.device_id.clone(),
            key,
            None,
            self.csr_path.clone(),
            self.csr_template.clone(),
        )
        .await?;

        eprintln!("Public key:\n{pubkey_pem}\n");

        Ok(())
    }
}
