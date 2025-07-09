use anyhow::Context;
use camino::Utf8PathBuf;
use certificate::CsrTemplate;
use clap::ValueEnum;
use elliptic_curve::sec1::EncodedPoint;
use elliptic_curve::sec1::FromEncodedPoint;
use tedge_config::TEdgeConfig;
use tedge_p11_server::pkcs11::CreateKeyParams;
use tedge_p11_server::pkcs11::KeyTypeParams;

use crate::cli::common::Cloud;
use crate::command::Command;
use crate::log::MaybeFancy;

pub struct CreateKeyCmd {
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

    async fn execute(&self, config: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        let socket_path = &config.device.cryptoki.socket_path;
        let pkcs11client =
            tedge_p11_server::TedgeP11Client::with_ready_check(socket_path.as_std_path().into());
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
        let pubkey_der = pkcs11client.create_key(None, params)?;
        let pubkey_pem = match self.r#type {
            KeyType::Rsa => {
                let pubkey_pem = pem::Pem::new("PUBLIC KEY", pubkey_der);
                pem::encode(&pubkey_pem)
            }
            KeyType::Ec => {
                // convert ECPoint to ECPublicKey
                // DER encoding of ECPoint: RFC5480 section 2.2
                println!("{pubkey_der:?} ({})", pubkey_der.len());
                // we have a DER OCTET STRING here so first 2 bytes are DER tag + length
                let pubkey_pem = match self.curve {
                    256 => {
                        let ec_point = EncodedPoint::<p256::NistP256>::from_bytes(&pubkey_der[2..])
                            .context("Failed to parse EC point")?;
                        let pubkey =
                            elliptic_curve::PublicKey::<p256::NistP256>::from_encoded_point(
                                &ec_point,
                            )
                            .into_option()
                            .context("Failed to create EC pubkey from EncodedPoint")?;
                        let der = pubkey.to_sec1_bytes();

                        pem::Pem::new("PUBLIC KEY", der)
                    }
                    384 => {
                        let ec_point = EncodedPoint::<p384::NistP384>::from_bytes(&pubkey_der[2..])
                            .context("Failed to parse EC point")?;
                        let pubkey =
                            elliptic_curve::PublicKey::<p384::NistP384>::from_encoded_point(
                                &ec_point,
                            )
                            .into_option()
                            .context("Failed to create EC pubkey from EncodedPoint")?;
                        let der = pubkey.to_sec1_bytes();

                        pem::Pem::new("PUBLIC KEY", der)
                    }
                    _ => return Err(anyhow::anyhow!("aaaa").into()),
                };

                pem::encode(&pubkey_pem)
            }
        };

        eprintln!("New keypair was successfully created.");

        // use returned private key to create a CSR

        // isn't device_id the same as certificate_cn?
        let common_name = crate::certificate_cn(
            &config
                .device_cert_path(None::<&Cloud>)
                .unwrap()
                .to_path_buf(),
        )
        .await?;

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

        let cryptoki_config = config.device.cryptoki_config(None).unwrap().unwrap();
        let key = super::create_csr::Key::Cryptoki {
            config: cryptoki_config,
            privkey_label: Some(self.label.clone()),
            pubkey_pem: Some(pubkey_pem.clone()),
            sigalg: Some(sigalg),
        };
        let csr_path = config
            .device_csr_path(None::<&Cloud>)
            .unwrap()
            .to_path_buf();

        super::create_device_csr(common_name, key, None, csr_path, self.csr_template.clone())
            .await?;

        eprintln!("Public key:\n{pubkey_pem}\n");

        Ok(())
    }
}
