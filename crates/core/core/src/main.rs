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
    messages::core::{MqttPayload, RegisterSender},
};

mod actors;
mod messages;

// #[actix::main]
fn main() -> Result<(), anyhow::Error> {
    tedge_utils::logging::initialise_tracing_subscriber(true);

    let topics = vec!["#"].try_into().expect("a list of topic filters");

    let mqtt_config = Config::new("localhost", 1883).with_subscriptions(dbg!(topics));

    let tedge_config_location = tedge_config::TEdgeConfigLocation::from_custom_root("/etc/tedge");
    let config = tedge_config::TEdgeConfigRepository::new(tedge_config_location).load()?;

    let sys = actix::System::new();
    sys.block_on(async move {
        debug!("Connecting");

        let mut http_proxy = JwtAuthHttpProxy::try_new(&config).await.unwrap();
        http_proxy.init().await.unwrap();

        // let channel = mqtt.sub_channel();
        let addr_sender = MqttSenderActor {}.start();
        let addr_cumulocity = CumulocityActor::try_new(config, http_proxy)
            .unwrap()
            .start();
        let addr_core = AgentCoreProducerActor::new(addr_cumulocity).start();

        addr_core.do_send(RegisterSender::new(addr_sender.recipient()));
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
