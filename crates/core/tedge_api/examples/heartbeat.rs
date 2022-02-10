use async_trait::async_trait;
use tedge_api::{
    address::EndpointKind,
    messages::{CoreMessageKind, PluginMessageKind},
    plugins::Comms,
    Address, CoreMessage, Plugin, PluginBuilder, PluginConfiguration, PluginError, PluginMessage,
};

struct HeartbeatServiceBuilder;

#[async_trait]
impl PluginBuilder for HeartbeatServiceBuilder {
    fn kind_name(&self) -> &'static str {
        todo!()
    }

    async fn verify_configuration(
        &self,
        _config: &PluginConfiguration,
    ) -> Result<(), tedge_api::errors::PluginError> {
        Ok(())
    }

    async fn instantiate(
        &self,
        config: PluginConfiguration,
        tedge_comms: tedge_api::plugins::Comms,
    ) -> Result<Box<dyn Plugin>, PluginError> {
        let hb_config: HeartbeatConfig = toml::Value::try_into(config.into_inner())?;
        Ok(Box::new(HeartbeatService::new(tedge_comms, hb_config)))
    }
}

#[derive(serde::Deserialize, Debug)]
struct HeartbeatConfig {
    interval: u64,
}

struct HeartbeatService {
    comms: tedge_api::plugins::Comms,
    config: HeartbeatConfig,
}

impl HeartbeatService {
    fn new(comms: tedge_api::plugins::Comms, config: HeartbeatConfig) -> Self {
        Self { comms, config }
    }
}

#[async_trait]
impl Plugin for HeartbeatService {
    async fn setup(&mut self) -> Result<(), PluginError> {
        println!("Setting up heartbeat service with interval: {}!", self.config.interval);
        Ok(())
    }

    async fn handle_message(&self, message: PluginMessage) -> Result<(), PluginError> {
        match message.kind() {
            tedge_api::messages::PluginMessageKind::CheckReadyness => {
                let msg = CoreMessage::new(
                    message.origin().clone(),
                    CoreMessageKind::SignalPluginState {
                        state: String::from("Ok"),
                    },
                );
                self.comms.send(msg).await?;
            }
        }

        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), PluginError> {
        println!("Shutting down heartbeat service!");
        Ok(())
    }
}

#[tokio::main]
async fn main() {
    let hsb = HeartbeatServiceBuilder;
    let (sender, mut receiver) = tokio::sync::mpsc::channel(10);

    let comms = Comms::new(sender);

    let config = toml::from_str(
        r#"
    interval = 200
    "#,
    )
    .unwrap();

    let mut heartbeat = hsb.instantiate(config, comms).await.unwrap();

    heartbeat.setup().await.unwrap();

    let handle = tokio::task::spawn(async move {
        let hb = heartbeat;

        hb.handle_message(PluginMessage::new(
            Address::new(EndpointKind::Core),
            PluginMessageKind::CheckReadyness,
        ))
        .await
        .unwrap();

        hb
    });

    println!(
        "Receiving message from service: {:#?}",
        receiver.recv().await
    );

    let mut heartbeat = handle.await.unwrap();

    heartbeat.shutdown().await.unwrap();
}
