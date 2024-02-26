use async_trait::async_trait;
use certificate::parse_root_certificate::create_tls_config;
use futures::SinkExt;
use futures::StreamExt;
use rumqttc::AsyncClient;
use rumqttc::Event;
use rumqttc::EventLoop;
use rumqttc::Incoming;
use rumqttc::MqttOptions;
use rumqttc::Outgoing;
use rumqttc::PubAck;
use rumqttc::PubRec;
use rumqttc::Publish;
use rumqttc::Transport;
use std::borrow::Cow;
use std::collections::HashMap;
use std::convert::Infallible;
use tedge_actors::futures::channel::mpsc;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tracing::error;

pub type MqttConfig = mqtt_channel::Config;
pub type MqttMessage = Message;

pub use mqtt_channel::DebugPayload;
pub use mqtt_channel::Message;
pub use mqtt_channel::MqttError;
pub use mqtt_channel::QoS;
pub use mqtt_channel::Topic;
pub use mqtt_channel::TopicFilter;
use tedge_config::TEdgeConfig;

pub struct MqttBridgeActorBuilder {
    signal_sender: mpsc::Sender<RuntimeRequest>,
}

impl MqttBridgeActorBuilder {
    pub async fn new(tedge_config: &TEdgeConfig, cloud_topics: &[impl AsRef<str>]) -> Self {
        let tls_config = create_tls_config(
            tedge_config.c8y.root_cert_path.clone().into(),
            tedge_config.device.key_path.clone().into(),
            tedge_config.device.cert_path.clone().into(),
        )
            .unwrap();
        let (signal_sender, _signal_receiver) = mpsc::channel(10);

        // TODO move this somewhere sensible, and make sure we validate it
        let prefix = std::env::var("TEDGE_BRIDGE_PREFIX").unwrap_or_else(|_| "c8y".to_owned());
        let mut local_config = MqttOptions::new(
            format!("tedge-mapper-bridge-{prefix}"),
            &tedge_config.mqtt.client.host,
            tedge_config.mqtt.client.port.into(),
        );
        // TODO cope with secured mosquitto
        local_config.set_manual_acks(true);
        let mut cloud_config = MqttOptions::new(
            tedge_config.device.id.try_read(tedge_config).unwrap(),
            tedge_config
                .c8y
                .mqtt
                .or_config_not_set()
                .unwrap()
                .host()
                .to_string(),
            8883,
        );
        cloud_config.set_manual_acks(true);

        cloud_config.set_transport(Transport::tls_with_config(tls_config.into()));
        let topic_prefix = format!("{prefix}/");
        let (local_client, local_event_loop) = AsyncClient::new(local_config, 10);
        let (cloud_client, cloud_event_loop) = AsyncClient::new(cloud_config, 10);

        // TODO support non c8y clouds
        local_client
            .subscribe(format!("{topic_prefix}#"), QoS::AtLeastOnce)
            .await
            .unwrap();
        for topic in cloud_topics {
            cloud_client
                .subscribe(topic.as_ref(), QoS::AtLeastOnce)
                .await
                .unwrap();
        }

        let (tx_pubs_from_cloud, rx_pubs_from_cloud) = mpsc::channel(10);
        let (tx_pubs_from_local, rx_pubs_from_local) = mpsc::channel(10);
        tokio::spawn(one_way_bridge(
            local_event_loop,
            cloud_client,
            move |topic| topic.strip_prefix(&topic_prefix).unwrap().into(),
            tx_pubs_from_local,
            rx_pubs_from_cloud,
        ));
        tokio::spawn(one_way_bridge(
            cloud_event_loop,
            local_client,
            move |topic| format!("{prefix}/{topic}").into(),
            tx_pubs_from_cloud,
            rx_pubs_from_local,
        ));

        Self { signal_sender }
    }

    pub(crate) fn build_actor(self) -> MqttBridgeActor {
        MqttBridgeActor {}
    }
}

async fn one_way_bridge<F: for<'a> Fn(&'a str) -> Cow<'a, str>>(
    mut recv_event_loop: EventLoop,
    target: AsyncClient,
    transform_topic: F,
    mut tx_pubs: mpsc::Sender<Publish>,
    mut rx_pubs: mpsc::Receiver<Publish>,
) {
    let mut forward_pkid_to_received_msg = HashMap::new();
    loop {
        let notification = match recv_event_loop.poll().await {
            Ok(notification) => notification,
            Err(err) => {
                error!("MQTT bridge connection error: {err}");
                continue;
            }
        };
        match notification {
            // Forwarding messages from event loop to target
            Event::Incoming(Incoming::Publish(publish)) => {
                target
                    .publish(
                        transform_topic(&publish.topic),
                        publish.qos,
                        publish.retain,
                        publish.payload.clone(),
                    )
                    .await
                    .unwrap();
                tx_pubs.send(publish).await.unwrap();
            }
            // Forwarding acks from event loop to target
            Event::Incoming(
                Incoming::PubAck(PubAck { pkid: ack_pkid })
                | Incoming::PubRec(PubRec { pkid: ack_pkid }),
            ) => {
                target
                    .ack(&forward_pkid_to_received_msg.remove(&ack_pkid).unwrap())
                    .await
                    .unwrap();
            }
            Event::Outgoing(Outgoing::Publish(pkid)) => {
                if let Some(msg) = rx_pubs.next().await {
                    forward_pkid_to_received_msg.insert(pkid, msg);
                } else {
                    break;
                }
            }
            _ => {}
        }
    }
}

impl Builder<MqttBridgeActor> for MqttBridgeActorBuilder {
    type Error = Infallible;

    fn try_build(self) -> Result<MqttBridgeActor, Self::Error> {
        Ok(self.build())
    }

    fn build(self) -> MqttBridgeActor {
        self.build_actor()
    }
}

impl RuntimeRequestSink for MqttBridgeActorBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        Box::new(self.signal_sender.clone())
    }
}

pub struct MqttBridgeActor {}

#[async_trait]
impl Actor for MqttBridgeActor {
    fn name(&self) -> &str {
        "MQTT-Bridge"
    }

    async fn run(mut self) -> Result<(), RuntimeError> {
        Ok(())
    }
}
