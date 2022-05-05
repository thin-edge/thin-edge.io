use std::{collections::HashMap, time::Duration};

use async_trait::async_trait;
use tedge_api::{
    address::ReplySenderFor,
    message::{AnyMessage, MessageType},
    plugin::{AnyMessages, BuiltPlugin, Handle, Message, PluginDeclaration, PluginExt},
    Address, CancellationToken, Plugin, PluginBuilder, PluginConfiguration, PluginDirectory,
    PluginError,
};

/// A message that represents a heartbeat that gets sent to plugins
#[derive(Debug)]
struct Heartbeat;
impl Message for Heartbeat {}

#[derive(Debug)]
struct RandomData;
impl Message for RandomData {}

/// A PluginBuilder that gets used to build a HeartbeatService plugin instance
#[derive(Debug)]
struct HeartbeatServiceBuilder;

#[derive(miette::Diagnostic, thiserror::Error, Debug)]
enum HeartbeatBuildError {
    #[error(transparent)]
    TomlParse(#[from] toml::de::Error),
}

#[async_trait]
impl<PD: PluginDirectory> PluginBuilder<PD> for HeartbeatServiceBuilder {
    fn kind_name() -> &'static str {
        todo!()
    }

    fn kind_message_types() -> tedge_api::plugin::HandleTypes
    where
        Self: Sized,
    {
        HeartbeatService::get_handled_types()
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
        let hb_config: HeartbeatConfig =
            toml::Value::try_into(config).map_err(HeartbeatBuildError::from)?;
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
        .finish())
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
    async fn start(&mut self) -> Result<(), PluginError> {
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
                    service.1.send_and_wait(Heartbeat).await.unwrap();
                    service.1.send_and_wait(RandomData).await.unwrap();
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
struct LogServiceBuilder;

// declare a set of messages that the CriticalService can receive.
// In this example, it can only receive a Heartbeat.
tedge_api::make_receiver_bundle!(struct HeartbeatMessages(Heartbeat, RandomData));

#[async_trait]
impl<PD: PluginDirectory> PluginBuilder<PD> for LogServiceBuilder {
    fn kind_name() -> &'static str {
        todo!()
    }

    fn kind_message_types() -> tedge_api::plugin::HandleTypes
    where
        Self: Sized,
    {
        LogService::get_handled_types()
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
        Ok(LogService {}.finish())
    }
}

/// The actual "critical" plugin implementation
struct LogService {}

/// The CriticalService can receive Heartbeat objects, thus it needs a Handle<Heartbeat>
/// implementation
#[async_trait]
impl Handle<AnyMessage> for LogService {
    async fn handle_message(
        &self,
        message: AnyMessage,
        _sender: ReplySenderFor<AnyMessage>,
    ) -> Result<(), PluginError> {
        println!("LogService: Received Message: {:?}", message);
        Ok(())
    }
}

impl PluginDeclaration for LogService {
    type HandledMessages = AnyMessages;
}

/// Because the CriticalService is of course a Plugin, it needs an implementation for that as well.
#[async_trait]
impl Plugin for LogService {
    async fn start(&mut self) -> Result<(), PluginError> {
        println!("CriticalService: Setting up critical service!");
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), PluginError> {
        println!("CriticalService: Shutting down critical service service!");
        Ok(())
    }
}

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
    types: Vec<MessageType>,
    receiver: Option<tedge_api::address::MessageReceiver>,
    sender: tedge_api::address::MessageSender,
}

/// The type that provides the communication infrastructure to the plugins.
#[derive(Debug)]
struct Communication {
    plugins: HashMap<String, PluginInfo>,
}

impl Communication {
    pub fn declare_plugin<PB: PluginBuilder<Self>>(&mut self, name: &str) {
        let (sender, receiver) = tokio::sync::mpsc::channel(10);
        self.plugins.insert(
            name.to_owned(),
            PluginInfo {
                types: PB::kind_message_types().into_types(),
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
        let asked_types: Vec<_> = MB::get_ids().into_iter().collect();

        let plug = self.plugins.get(name).unwrap_or_else(|| {
            // This is an example, so we panic!() here.
            // In real-world, we would do some reporting and return an error
            panic!(
                "Didn't find plugin with name {}, got: {:?}",
                name,
                self.plugins.keys().collect::<Vec<_>>()
            )
        });

        if !asked_types
            .iter()
            .all(|req_type| plug.types.iter().any(|ty| ty.satisfy(req_type)))
        {
            // This is an example, so we panic!() here
            // In real-world, we would do some reporting and return an error
            panic!(
                "Asked for {:#?} but plugin {} only has types {:#?}",
                asked_types, name, plug.types,
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
    let csb = LogServiceBuilder;

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
    comms.declare_plugin::<LogServiceBuilder>("critical-service");
    comms.declare_plugin::<HeartbeatServiceBuilder>("heartbeat");

    // The following would all be handled by the core implementation, a main() author would only
    // need to call some kind of "run everything" function

    let cancel_token = CancellationToken::new();

    let mut heartbeat = build_heartbeat_plugin(&mut comms, cancel_token.child_token()).await;
    let mut critical_service = build_critical_plugin(&mut comms, cancel_token.child_token()).await;

    heartbeat.plugin_mut().start().await.unwrap();
    critical_service.plugin_mut().start().await.unwrap();

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
