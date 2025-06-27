use clap::ValueEnum;
use tedge_config::TEdgeConfig;
use tedge_p11_server::pkcs11::{CreateKeyParams, KeyTypeParams};

use crate::command::Command;
use crate::log::MaybeFancy;

pub struct CreateKeyCmd {
    pub bits: u16,
    pub curve: u16,
    pub label: String,
    pub r#type: KeyType,
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
        pkcs11client.create_key(None, params)?;
        eprintln!("New keypair was successfully created.");
        Ok(())
    }
}
