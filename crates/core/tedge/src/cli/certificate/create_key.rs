use tedge_config::TEdgeConfig;
use tedge_p11_server::pkcs11::{CreateKeyParams, KeyTypeParams};

use crate::command::Command;
use crate::log::MaybeFancy;

pub struct CreateKeyCmd {
    pub bits: u16,
    pub label: String,
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
        let params = CreateKeyParams {
            key: KeyTypeParams::Rsa { bits: self.bits },
            token: None,
            label: self.label.clone(),
        };
        pkcs11client.create_key(None, params)?;
        eprintln!("New keypair was successfully created.");
        Ok(())
    }
}
