use async_trait::async_trait;
use c8y_api::utils::bridge::main_device_health_topic;
use c8y_api::utils::bridge::C8Y_BRIDGE_DOWN_PAYLOAD;
use c8y_api::utils::bridge::C8Y_BRIDGE_UP_PAYLOAD;
use certificate::parse_root_certificate::create_tls_config;
use futures::SinkExt;
use futures::StreamExt;
use rumqttc::AsyncClient;
use rumqttc::Event;
use rumqttc::EventLoop;
use rumqttc::Incoming;
use rumqttc::LastWill;
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
use tracing::log::info;

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
    pub async fn new(
        tedge_config: &TEdgeConfig,
        service_name: String,
        cloud_topics: &[impl AsRef<str>],
    ) -> Self {
        let tls_config = create_tls_config(
            tedge_config.c8y.root_cert_path.clone().into(),
            tedge_config.device.key_path.clone().into(),
            tedge_config.device.cert_path.clone().into(),
        )
        .unwrap();
        let (signal_sender, _signal_receiver) = mpsc::channel(10);

        let prefix = tedge_config.c8y.bridge.topic_prefix.clone();
        let mut local_config = MqttOptions::new(
            &service_name,
            &tedge_config.mqtt.client.host,
            tedge_config.mqtt.client.port.into(),
        );
        let health_topic = main_device_health_topic(&service_name);
        // TODO cope with secured mosquitto
        local_config.set_manual_acks(true);
        // TODO const for payload
        local_config.set_last_will(LastWill::new(
            &health_topic,
            C8Y_BRIDGE_DOWN_PAYLOAD,
            QoS::AtLeastOnce,
            true,
        ));
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

        let [msgs_local, msgs_cloud] = bidirectional_channel(10);
        tokio::spawn(half_bridge(
            local_event_loop,
            cloud_client,
            move |topic| topic.strip_prefix(&topic_prefix).unwrap().into(),
            msgs_local,
            None,
        ));
        tokio::spawn(half_bridge(
            cloud_event_loop,
            local_client,
            move |topic| format!("{prefix}/{topic}").into(),
            msgs_cloud,
            Some(health_topic),
        ));

        Self { signal_sender }
    }

    pub(crate) fn build_actor(self) -> MqttBridgeActor {
        MqttBridgeActor {}
    }
}

fn bidirectional_channel<T>(buffer: usize) -> [BidirectionalChannelHalf<T>; 2] {
    let (tx_first, rx_first) = mpsc::channel(buffer);
    let (tx_second, rx_second) = mpsc::channel(buffer);
    [
        BidirectionalChannelHalf {
            tx: tx_first,
            rx: rx_second,
        },
        BidirectionalChannelHalf {
            tx: tx_second,
            rx: rx_first,
        },
    ]
}

struct BidirectionalChannelHalf<T> {
    tx: mpsc::Sender<T>,
    rx: mpsc::Receiver<T>,
}

impl<'a, T> BidirectionalChannelHalf<T> {
    pub fn send(&'a mut self, item: T) -> futures::sink::Send<'a, mpsc::Sender<T>, T> {
        self.tx.send(item)
    }

    pub fn recv(&mut self) -> futures::stream::Next<mpsc::Receiver<T>> {
        self.rx.next()
    }
}

async fn half_bridge<F: for<'a> Fn(&'a str) -> Cow<'a, str>>(
    mut recv_event_loop: EventLoop,
    target: AsyncClient,
    transform_topic: F,
    mut corresponding_bridge_half: BidirectionalChannelHalf<Option<Publish>>,
    health_topic: Option<String>,
) {
    let mut forward_pkid_to_received_msg = HashMap::new();
    let mut last_err = Some("dummy error".into());

    loop {
        let notification = match recv_event_loop.poll().await {
            // TODO notify if this is us recovering from an error
            Ok(notification) => {
                if last_err.as_ref().is_some() {
                    // TODO clarify whether this is cloud/local
                    info!("MQTT bridge connected");
                    last_err = None;
                    if let Some(health_topic) = &health_topic {
                        target
                            .publish(health_topic, QoS::AtLeastOnce, true, C8Y_BRIDGE_UP_PAYLOAD)
                            .await
                            .unwrap();
                        corresponding_bridge_half.send(None).await.unwrap();
                    }
                }
                notification
            }
            Err(err) => {
                let err = err.to_string();
                if last_err.as_ref() != Some(&err) {
                    // TODO clarify whether this is cloud/local
                    error!("MQTT bridge connection error: {err}");
                    last_err = Some(err);
                    if let Some(health_topic) = &health_topic {
                        target
                            .publish(
                                health_topic,
                                QoS::AtLeastOnce,
                                true,
                                C8Y_BRIDGE_DOWN_PAYLOAD,
                            )
                            .await
                            .unwrap();
                        corresponding_bridge_half.send(None).await.unwrap();
                    }
                }
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
                corresponding_bridge_half.send(Some(publish)).await.unwrap();
            }
            // Forwarding acks from event loop to target
            Event::Incoming(
                Incoming::PubAck(PubAck { pkid: ack_pkid })
                | Incoming::PubRec(PubRec { pkid: ack_pkid }),
            ) => {
                if let Some(msg) = forward_pkid_to_received_msg.remove(&ack_pkid).unwrap() {
                    target.ack(&msg).await.unwrap();
                }
            }
            Event::Outgoing(Outgoing::Publish(pkid)) => {
                match corresponding_bridge_half.recv().await {
                    Some(optional_msg) => {
                        forward_pkid_to_received_msg.insert(pkid, optional_msg);
                    }
                    None => break,
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
