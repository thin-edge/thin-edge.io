use actix::prelude::*;
use anyhow::Result;
use c8y_api::http_proxy::{C8YHttpProxy, JwtAuthHttpProxy};
use futures::StreamExt;
use mqtt_channel::{Config, Connection};
use tedge_config::ConfigRepository;
use tracing::debug;

use crate::{
    actors::{
        c8y::CumulocityActor,
        core::{AgentCoreProducerActor, MqttSenderActor},
    },
    errors::PluginError,
    messages::core::{MqttPayload, RegisterSender},
};

mod actors;
mod errors;
mod messages;

pub type PluginConfiguration = toml::Spanned<toml::value::Value>;

#[derive(serde::Deserialize, Debug)]
struct HttpStopConfig {
    bind: std::net::SocketAddr,
}

// #[actix::main]
fn main() -> Result<(), anyhow::Error> {
    tedge_utils::logging::initialise_tracing_subscriber(true);

    // General configuration for actors/plugins is loaded from a generic file (currently hardcoded to be taken from repo path).
    // These configs can be used to setup actors/plugins for specific features, eg listening ports, options enablement etc.
    let config_bytes = std::fs::read("/home/makrist/thin-edge.io/example-config.toml")?;
    let config: PluginConfiguration = toml::from_slice(config_bytes.as_slice())?;
    dbg!(&config);

    let config_stop = config
        .get_ref()
        .clone()
        .try_into::<HttpStopConfig>()
        .map_err(|_| anyhow::anyhow!("Failed to parse configuration"))?;

    dbg!(&config_stop);

    // To adhere to current architecture the configuration for a message producer uses current tedge config implementation,
    // threfore needs to be read as it is done in any other binary in tedge.
    let topics = vec!["#"].try_into().expect("a list of topic filters");

    let mqtt_config = Config::new("localhost", 1883).with_subscriptions(dbg!(topics));

    let tedge_config_location = tedge_config::TEdgeConfigLocation::from_custom_root("/etc/tedge");
    let config = tedge_config::TEdgeConfigRepository::new(tedge_config_location).load()?;

    // Create actix runtime which is using tokio.
    let sys = actix::System::new();
    sys.block_on(async move {
        debug!("Connecting");

        let mut http_proxy = JwtAuthHttpProxy::try_new(&config).await.unwrap();
        http_proxy.init().await.unwrap();

        // Start creating actors to handle features:
        // MqttSenderActor is just a prototype atm, but its intend is to send output MQTT messages to the server.
        let addr_sender = MqttSenderActor {}.start();

        // CumulocityActor is responsible for processing messages for c8y.
        let addr_cumulocity = CumulocityActor::try_new(config, http_proxy)
            .unwrap()
            .start();
        let addr_core = AgentCoreProducerActor::new(addr_cumulocity).start();

        addr_core.do_send(RegisterSender::new(addr_sender.recipient()));

        // Main producer loop, requires an input for incoming data, probably requires rethinking how do we want to run.
        let _producer_handle = tokio::spawn(async move {
            let mut mqtt = Connection::new(&mqtt_config).await.unwrap();

            while let Some(msg) = mqtt.received.next().await {
                debug!("Request {:?}", msg);
                let mqtt_msg = MqttPayload::new(msg);
                addr_core.do_send(mqtt_msg);
            }
        });
    });
    sys.run()?;

    Ok(())
}
