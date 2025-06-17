use clap::ValueEnum;
use tedge_config::TEdgeConfig;
use tedge_p11_server::pkcs11::CreateKeyParams;
use tedge_p11_server::pkcs11::KeyTypeParams;
use tedge_p11_server::CryptokiConfig;

use crate::command::Command;
use crate::log::MaybeFancy;

pub struct CreateKeyCmd {
    pub cryptoki_config: CryptokiConfig,
    pub bits: RsaBits,
    pub curve: EcCurve,
    pub label: String,
    pub r#type: KeyType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum KeyType {
    Rsa,
    Ec,
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

    async fn execute(&self, _config: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        let key = match self.r#type {
            KeyType::Rsa => KeyTypeParams::Rsa {
                bits: self.bits.into(),
            },
            KeyType::Ec => KeyTypeParams::Ec {
                curve: self.curve.into(),
            },
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
        eprintln!("Public key:\n{pubkey_pem}\n");

        Ok(())
    }
}
