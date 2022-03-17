use std::{
    any::TypeId,
    collections::{HashMap, HashSet},
};

use async_trait::async_trait;
use tedge_api::{
    plugin::{BuiltPlugin, Handle, HandleTypes, Message, PluginExt},
    Address, CoreCommunication, Plugin, PluginBuilder, PluginConfiguration, PluginError,
};

#[derive(Debug)]
struct Heartbeat;
impl Message for Heartbeat {}

#[derive(Debug)]
enum HeartbeatStatusReply {
    Alive,
    Degraded,
}
impl Message for HeartbeatStatusReply {}

#[derive(Debug)]
struct HeartbeatServiceBuilder;

type HeartbeatMessages = (HeartbeatStatusReply,);

#[async_trait]
impl<CC: CoreCommunication> PluginBuilder<CC> for HeartbeatServiceBuilder {
    fn kind_name(&self) -> &'static str {
        todo!()
    }

    fn kind_message_types() -> tedge_api::plugin::HandleTypes
    where
        Self: Sized,
    {
        HandleTypes::get_handlers_for::<(HeartbeatStatusReply,), HeartbeatService>()
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
        tedge_comms: &CC,
    ) -> Result<BuiltPlugin, PluginError>
    where
        CC: 'async_trait,
    {
        let hb_config: HeartbeatConfig = toml::Value::try_into(config.into_inner())?;
        let monitored_services = hb_config
            .plugins
            .iter()
            .map(|name| tedge_comms.get_address_for::<CriticalServiceMessage>(name))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(
            HeartbeatService::new(hb_config, monitored_services)
                .into_untyped::<HeartbeatMessages>(),
        )
    }
}

#[derive(serde::Deserialize, Debug)]
struct HeartbeatConfig {
    interval: u64,
    plugins: Vec<String>,
}

struct HeartbeatService {
    config: HeartbeatConfig,
    monitored_services: Vec<Address<CriticalServiceMessage>>,
}

#[async_trait]
impl Plugin for HeartbeatService {
    async fn setup(&mut self) -> Result<(), PluginError> {
        println!(
            "Setting up heartbeat service with interval: {}!",
            self.config.interval
        );

        for service in &self.monitored_services {
            println!("Sending heartbeat to service");
            service.send(Heartbeat).await.unwrap();
        }
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), PluginError> {
        println!("Shutting down heartbeat service!");
        Ok(())
    }
}

impl HeartbeatService {
    fn new(
        config: HeartbeatConfig,
        monitored_services: Vec<Address<CriticalServiceMessage>>,
    ) -> Self {
        Self {
            config,
            monitored_services,
        }
    }
}

#[async_trait]
impl Handle<HeartbeatStatusReply> for HeartbeatService {
    async fn handle_message(&self, _message: HeartbeatStatusReply) -> Result<(), PluginError> {
        println!("Received HeartbeatReply!");
        Ok(())
    }
}

struct CriticalServiceBuilder;

tedge_api::make_message_bundle!(struct CriticalServiceMessage(Heartbeat));

#[async_trait]
impl<CC: CoreCommunication> PluginBuilder<CC> for CriticalServiceBuilder {
    fn kind_name(&self) -> &'static str {
        todo!()
    }

    fn kind_message_types() -> tedge_api::plugin::HandleTypes
    where
        Self: Sized,
    {
        HandleTypes::get_handlers_for::<(Heartbeat,), CriticalService>()
    }

    async fn verify_configuration(
        &self,
        _config: &PluginConfiguration,
    ) -> Result<(), tedge_api::error::PluginError> {
        Ok(())
    }

    async fn instantiate(
        &self,
        _config: PluginConfiguration,
        _tedge_comms: &CC,
    ) -> Result<BuiltPlugin, PluginError>
    where
        CC: 'async_trait,
    {
        Ok(CriticalService {}.into_untyped::<(Heartbeat,)>())
    }
}

struct CriticalService;

#[async_trait]
impl Handle<Heartbeat> for CriticalService {
    async fn handle_message(&self, _message: Heartbeat) -> Result<(), PluginError> {
        println!("Received Heartbeat!");
        Ok(())
    }
}

#[async_trait]
impl Plugin for CriticalService {
    async fn setup(&mut self) -> Result<(), PluginError> {
        println!("Setting up critical service!");
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), PluginError> {
        println!("Shutting down critical service service!");
        Ok(())
    }
}

#[derive(Debug)]
struct PluginInfo {
    types: HashSet<(&'static str, TypeId)>,
    receiver: Option<tedge_api::address::MessageReceiver>,
    sender: tedge_api::address::MessageSender,
}

impl Clone for PluginInfo {
    fn clone(&self) -> Self {
        PluginInfo {
            types: self.types.clone(),
            receiver: None,
            sender: self.sender.clone(),
        }
    }
}

#[derive(Clone, Debug)]
struct Communication {
    plugins: HashMap<String, PluginInfo>,
}

impl Communication {
    pub fn declare_plugin<PB: PluginBuilder<Self>>(&mut self, name: &str) {
        let (sender, receiver) = tokio::sync::mpsc::channel(10);
        self.plugins.insert(
            name.to_owned(),
            PluginInfo {
                types: PB::kind_message_types().into(),
                sender,
                receiver: Some(receiver),
            },
        );
    }
}

impl CoreCommunication for Communication {
    fn get_address_for<MB: tedge_api::plugin::MessageBundle>(
        &self,
        name: &str,
    ) -> Result<Address<MB>, PluginError> {
        let types = MB::get_ids().into_iter().collect();

        let plug = self.plugins.get(name).unwrap_or_else(|| {
            panic!(
                "Didn't find plugin with name {}, got: {:?}",
                name,
                self.plugins.keys().collect::<Vec<_>>()
            )
        });

        if !plug.types.is_superset(&types) {
            panic!(
                "Asked for {:#?} but plugin {} only has types {:#?}",
                types, name, plug.types,
            );
        } else {
            Ok(Address::new(plug.sender.clone()))
        }
    }
}

async fn build_critical_plugin(comms: &mut Communication) -> BuiltPlugin {
    let csb = CriticalServiceBuilder;

    let config = toml::from_str("").unwrap();

    csb.instantiate(config, comms).await.unwrap()
}

async fn build_heartbeat_plugin(comms: &mut Communication) -> BuiltPlugin {
    let hsb = HeartbeatServiceBuilder;

    let config = toml::from_str(
        r#"
    interval = 200
    plugins = ["critical-service"]
    "#,
    )
    .unwrap();

    hsb.instantiate(config, comms).await.unwrap()
}

#[tokio::main]
async fn main() {
    let mut comms = Communication {
        plugins: HashMap::new(),
    };

    comms.declare_plugin::<CriticalServiceBuilder>("critical-service");
    comms.declare_plugin::<HeartbeatServiceBuilder>("heartbeat");

    let mut heartbeat = build_heartbeat_plugin(&mut comms).await;
    let mut critical_service = build_critical_plugin(&mut comms).await;

    heartbeat.plugin_mut().setup().await.unwrap();
    critical_service.plugin_mut().setup().await.unwrap();

    let mut recv = comms
        .plugins
        .get_mut("heartbeat")
        .unwrap()
        .receiver
        .take()
        .unwrap();

    let hb_handle = tokio::task::spawn(async move {
        let hb = heartbeat;

        for msg in recv.recv().await {
            hb.handle_message(msg).await.unwrap();
        }

        hb
    });

    let mut recv = comms
        .plugins
        .get_mut("critical-service")
        .unwrap()
        .receiver
        .take()
        .unwrap();

    let cs_handle = tokio::task::spawn(async move {
        let cs = critical_service;

        for msg in recv.recv().await {
            println!("Critical service received message!");
            cs.handle_message(msg).await.unwrap();
        }

        cs
    });

    let (heartbeat, critical_service) = tokio::join!(hb_handle, cs_handle);

    heartbeat.unwrap().plugin_mut().shutdown().await.unwrap();
    critical_service
        .unwrap()
        .plugin_mut()
        .shutdown()
        .await
        .unwrap();
}
