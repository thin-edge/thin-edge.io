use async_trait::async_trait;
use tedge_api::{
    address::EndpointKind, Address, CoreCommunication, Message, MessageKind, Plugin, PluginBuilder,
    PluginConfiguration, PluginError,
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
    ) -> Result<(), tedge_api::error::PluginError> {
        Ok(())
    }

    async fn instantiate(
        &self,
        config: PluginConfiguration,
        tedge_comms: tedge_api::plugin::CoreCommunication,
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
    comms: tedge_api::plugin::CoreCommunication,
    config: HeartbeatConfig,
}

impl HeartbeatService {
    fn new(comms: tedge_api::plugin::CoreCommunication, config: HeartbeatConfig) -> Self {
        Self { comms, config }
    }
}

#[async_trait]
impl Plugin for HeartbeatService {
    async fn setup(&mut self) -> Result<(), PluginError> {
        println!(
            "Setting up heartbeat service with interval: {}!",
            self.config.interval
        );
        Ok(())
    }

    async fn handle_message(&self, message: Message) -> Result<(), PluginError> {
        match message.kind() {
            MessageKind::CheckReadyness => {
                let kind = MessageKind::SignalPluginState {
                    state: String::from("Ok"),
                };
                self.comms.send(kind, message.origin().clone()).await?;
            }
            msg => println!("Does not handle: {:#?}", msg),
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

    let plugin_name = "heartbeat-service".to_string();
    let comms = CoreCommunication::new(plugin_name.clone(), sender);

    let config = toml::from_str(
        r#"
    interval = 200
    "#,
    )
    .unwrap();

    let mut heartbeat = hsb.instantiate(config, comms.clone()).await.unwrap();

    heartbeat.setup().await.unwrap();

    let handle = tokio::task::spawn(async move {
        let hb = heartbeat;

        hb.handle_message(Message::new(
            Address::new(EndpointKind::Plugin { id: plugin_name }),
            Address::new(EndpointKind::Core),
            MessageKind::CheckReadyness,
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
