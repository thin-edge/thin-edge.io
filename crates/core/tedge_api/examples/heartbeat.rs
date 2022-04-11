use std::{
    any::TypeId,
    collections::{HashMap, HashSet},
    time::Duration,
};

use async_trait::async_trait;
use futures::FutureExt;
use tedge_api::{
    address::ReplySender,
    message::NoReply,
    plugin::{BuiltPlugin, Handle, HandleTypes, Message, PluginDeclaration, PluginExt},
    Address, CancellationToken, Plugin, PluginBuilder, PluginConfiguration, PluginDirectory,
    PluginError,
};

/// A message that represents a heartbeat that gets sent to plugins
#[derive(Debug)]
struct Heartbeat;
impl Message for Heartbeat {
    type Reply = HeartbeatStatus;
}

/// The reply for a heartbeat
#[derive(Debug)]
enum HeartbeatStatus {
    Alive,
    Degraded,
}
impl Message for HeartbeatStatus {
    type Reply = NoReply;
}

/// A PluginBuilder that gets used to build a HeartbeatService plugin instance
#[derive(Debug)]
struct HeartbeatServiceBuilder;

#[async_trait]
impl<PD: PluginDirectory> PluginBuilder<PD> for HeartbeatServiceBuilder {
    fn kind_name() -> &'static str {
        todo!()
    }

    fn kind_message_types() -> tedge_api::plugin::HandleTypes
    where
        Self: Sized,
    {
        HandleTypes::empty()
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
        cancellation_token: CancellationToken,
        plugin_dir: &PD,
    ) -> Result<BuiltPlugin, PluginError>
    where
        PD: 'async_trait,
    {
        let hb_config: HeartbeatConfig = toml::Value::try_into(config.into_inner())?;
        let monitored_services = hb_config
            .plugins
            .iter()
            .map(|name| {
                plugin_dir
                    .get_address_for::<HeartbeatMessages>(name)
                    .map(|addr| (name.clone(), addr))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(HeartbeatService::new(
            Duration::from_millis(hb_config.interval),
            monitored_services,
            cancellation_token,
        )
        .into_untyped())
    }
}

/// The configuration a HeartbeatServices can receive is represented by this type
#[derive(serde::Deserialize, Debug)]
struct HeartbeatConfig {
    interval: u64,
    plugins: Vec<String>,
}

/// The HeartbeatService type represents the actual plugin
struct HeartbeatService {
    interval_duration: Duration,
    monitored_services: Vec<(String, Address<HeartbeatMessages>)>,
    cancel_token: CancellationToken,
}

impl PluginDeclaration for HeartbeatService {
    type HandledMessages = ();
}

#[async_trait]
impl Plugin for HeartbeatService {
    /// The setup function of the HeartbeatService can be used by the plugin author to setup for
    /// example a connection to an external service. In this example, it is simply used to send the
    /// heartbeat
    ///
    /// Because this example is _simple_, we do not spawn a background task that periodically sends
    /// the heartbeat. In a real world scenario, that background task would be started here.
    async fn setup(&mut self) -> Result<(), PluginError> {
        println!(
            "HeartbeatService: Setting up heartbeat service with interval: {:?}!",
            self.interval_duration
        );

        for service in &self.monitored_services {
            let mut interval = tokio::time::interval(self.interval_duration);
            let service = service.clone();
            let cancel_token = self.cancel_token.child_token();
            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = interval.tick() => {}
                        _ = cancel_token.cancelled() => {
                            break
                        }
                    }
                    println!(
                        "HeartbeatService: Sending heartbeat to service: {:?}",
                        service
                    );
                    tokio::select! {
                        reply = service
                        .1
                        .send(Heartbeat)
                        .then(|answer| {
                            answer.unwrap()
                            .wait_for_reply(Duration::from_millis(100))}
                        ) => {
                            match reply
                            {
                                Ok(HeartbeatStatus::Alive) => {
                                    println!("HeartbeatService: Received all is well!")
                                }
                                Ok(HeartbeatStatus::Degraded) => {
                                    println!(
                                        "HeartbeatService: Oh-oh! Plugin '{}' is not doing well",
                                        service.0
                                        )
                                }

                                Err(reply_error) => {
                                    println!(
                                        "HeartbeatService: Critical error for '{}'! {reply_error}",
                                        service.0
                                        )
                                }
                            }
                        }

                        _ = cancel_token.cancelled() => {
                            break
                        }
                    }
                }
            });
        }
        Ok(())
    }

    /// A plugin author can use this shutdown function to clean resources when thin-edge shuts down
    async fn shutdown(&mut self) -> Result<(), PluginError> {
        println!("HeartbeatService: Shutting down heartbeat service!");
        Ok(())
    }
}

impl HeartbeatService {
    fn new(
        interval_duration: Duration,
        monitored_services: Vec<(String, Address<HeartbeatMessages>)>,
        cancel_token: CancellationToken,
    ) -> Self {
        Self {
            interval_duration,
            monitored_services,
            cancel_token,
        }
    }
}

/// A plugin that receives heartbeats
struct CriticalServiceBuilder;

// declare a set of messages that the CriticalService can receive.
// In this example, it can only receive a Heartbeat.
tedge_api::make_receiver_bundle!(struct HeartbeatMessages(Heartbeat));

#[async_trait]
impl<PD: PluginDirectory> PluginBuilder<PD> for CriticalServiceBuilder {
    fn kind_name() -> &'static str {
        todo!()
    }

    fn kind_message_types() -> tedge_api::plugin::HandleTypes
    where
        Self: Sized,
    {
        CriticalService::get_handled_types()
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
        _cancellation_token: CancellationToken,
        _plugin_dir: &PD,
    ) -> Result<BuiltPlugin, PluginError>
    where
        PD: 'async_trait,
    {
        Ok(CriticalService {
            status: tokio::sync::Mutex::new(true),
        }
        .into_untyped())
    }
}

/// The actual "critical" plugin implementation
struct CriticalService {
    status: tokio::sync::Mutex<bool>,
}

/// The CriticalService can receive Heartbeat objects, thus it needs a Handle<Heartbeat>
/// implementation
#[async_trait]
impl Handle<Heartbeat> for CriticalService {
    async fn handle_message(
        &self,
        _message: Heartbeat,
        sender: ReplySender<HeartbeatStatus>,
    ) -> Result<(), PluginError> {
        println!("CriticalService: Received Heartbeat!");
        let mut status = self.status.lock().await;

        let _ = sender.reply(if *status {
            println!("CriticalService: Sending back alive!");
            HeartbeatStatus::Alive
        } else {
            println!("CriticalService: Sending back degraded!");
            HeartbeatStatus::Degraded
        });

        *status = !*status;
        Ok(())
    }
}

impl PluginDeclaration for CriticalService {
    type HandledMessages = (Heartbeat,);
}

/// Because the CriticalService is of course a Plugin, it needs an implementation for that as well.
#[async_trait]
impl Plugin for CriticalService {
    async fn setup(&mut self) -> Result<(), PluginError> {
        println!("CriticalService: Setting up critical service!");
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), PluginError> {
        println!("CriticalService: Shutting down critical service service!");
        Ok(())
    }
}

//
// The following pieces of code would be implemented by a "core" component, that is responsible for
// setting up plugins and their communication.
//
// Plugin authors do not need to write this code, but need a basic understanding what it does and
// how it works.
// As this is an example, we implement it here to showcase how it is done.
//

/// Helper type for keeping information about plugins during runtime
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

/// The type that provides the communication infrastructure to the plugins.
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

impl PluginDirectory for Communication {
    fn get_address_for<MB: tedge_api::address::ReceiverBundle>(
        &self,
        name: &str,
    ) -> Result<Address<MB>, tedge_api::error::DirectoryError> {
        let types = MB::get_ids().into_iter().collect();

        let plug = self.plugins.get(name).unwrap_or_else(|| {
            // This is an example, so we panic!() here.
            // In real-world, we would do some reporting and return an error
            panic!(
                "Didn't find plugin with name {}, got: {:?}",
                name,
                self.plugins.keys().collect::<Vec<_>>()
            )
        });

        if !plug.types.is_superset(&types) {
            // This is an example, so we panic!() here
            // In real-world, we would do some reporting and return an error
            panic!(
                "Asked for {:#?} but plugin {} only has types {:#?}",
                types, name, plug.types,
            );
        } else {
            Ok(Address::new(plug.sender.clone()))
        }
    }

    fn get_address_for_core(&self) -> Address<tedge_api::CoreMessages> {
        todo!()
    }
}

/// Helper function
async fn build_critical_plugin(
    comms: &mut Communication,
    cancel_token: CancellationToken,
) -> BuiltPlugin {
    let csb = CriticalServiceBuilder;

    let config = toml::from_str("").unwrap();

    csb.instantiate(config, cancel_token, comms).await.unwrap()
}

/// Helper function
async fn build_heartbeat_plugin(
    comms: &mut Communication,
    cancel_token: CancellationToken,
) -> BuiltPlugin {
    let hsb = HeartbeatServiceBuilder;

    let config = toml::from_str(
        r#"
    interval = 5000
    plugins = ["critical-service"]
    "#,
    )
    .unwrap();

    hsb.instantiate(config, cancel_token, comms).await.unwrap()
}

#[tokio::main]
async fn main() {
    // This implementation now ties everything together
    //
    // This would be implemented in a CLI binary using the "core" implementation to boot things up.
    //
    // Here, we just tie everything together in the minimal possible way, to showcase how such a
    // setup would basically work.

    let mut comms = Communication {
        plugins: HashMap::new(),
    };

    // in a main(), the core would be told what plugins are available.
    // This would, in a real-world scenario, not happen on the "communication" type directly.
    // Still, this needs to be done by a main()-author.
    comms.declare_plugin::<CriticalServiceBuilder>("critical-service");
    comms.declare_plugin::<HeartbeatServiceBuilder>("heartbeat");

    // The following would all be handled by the core implementation, a main() author would only
    // need to call some kind of "run everything" function

    let cancel_token = CancellationToken::new();

    let mut heartbeat = build_heartbeat_plugin(&mut comms, cancel_token.child_token()).await;
    let mut critical_service = build_critical_plugin(&mut comms, cancel_token.child_token()).await;

    heartbeat.plugin_mut().setup().await.unwrap();
    critical_service.plugin_mut().setup().await.unwrap();

    let mut recv = comms
        .plugins
        .get_mut("heartbeat")
        .unwrap()
        .receiver
        .take()
        .unwrap();

    let hb_cancel_token = cancel_token.child_token();
    let hb_handle = tokio::task::spawn(async move {
        let hb = heartbeat;

        loop {
            tokio::select! {
                Some(msg) = recv.recv() => {
                    hb.handle_message(msg).await.unwrap();
                }
                _ = hb_cancel_token.cancelled() => break,
            }
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

    let cs_cancel_token = cancel_token.child_token();
    let cs_handle = tokio::task::spawn(async move {
        let cs = critical_service;

        loop {
            tokio::select! {
                Some(msg) = recv.recv() => {
                    cs.handle_message(msg).await.unwrap();
                }
                _ = cs_cancel_token.cancelled() => break,
            }
        }

        cs
    });

    println!("Core: Stopping everything in 10 seconds!");
    tokio::time::sleep(Duration::from_secs(12)).await;

    println!("Core: SHUTTING DOWN");
    cancel_token.cancel();

    let (heartbeat, critical_service) = tokio::join!(hb_handle, cs_handle);

    heartbeat.unwrap().plugin_mut().shutdown().await.unwrap();
    critical_service
        .unwrap()
        .plugin_mut()
        .shutdown()
        .await
        .unwrap();

    println!("Core: Shut down");
}
