use camino::Utf8PathBuf;
use certificate::CsrTemplate;
use clap::ValueEnum;
use tedge_config::TEdgeConfig;
use tedge_p11_server::pkcs11::{CreateKeyParams, KeyTypeParams};
use tedge_p11_server::CryptokiConfig;

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

#[derive(Debug, Clone, PartialEq, Eq, ValueEnum)]
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
        let pkcs11client = tedge_p11_server::client::TedgeP11Client::with_ready_check(
            socket_path.as_std_path().into(),
        );
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
        // TODO: don't assume it's RSA
        let pubkey_der = pkcs11client.create_key(None, params)?;
        let pubkey_pem = pem::Pem::new("PUBLIC KEY", pubkey_der);
        let pubkey_pem = pem::encode(&pubkey_pem);

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

        let cryptoki_config = config.device.cryptoki_config(None).unwrap().unwrap();
        let key = super::create_csr::Key::Cryptoki {
            config: cryptoki_config,
            privkey_label: Some(self.label.clone()),
            pubkey_pem: Some(pubkey_pem.clone()),
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
