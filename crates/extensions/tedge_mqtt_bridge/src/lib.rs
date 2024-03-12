use async_trait::async_trait;
use certificate::parse_root_certificate::create_tls_config;
use futures::SinkExt;
use futures::StreamExt;
use rumqttc::AsyncClient;
use rumqttc::ConnectionError;
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
use tedge_actors::NullSender;
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
use tedge_api::main_device_health_topic;
use tedge_api::MQTT_BRIDGE_DOWN_PAYLOAD;
use tedge_api::MQTT_BRIDGE_UP_PAYLOAD;
use tedge_config::TEdgeConfig;

pub struct MqttBridgeActorBuilder {}

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

        let prefix = tedge_config.c8y.bridge.topic_prefix.clone();
        let mut local_config = MqttOptions::new(
            &service_name,
            &tedge_config.mqtt.client.host,
            tedge_config.mqtt.client.port.into(),
        );
        let health_topic = main_device_health_topic(&service_name);
        // TODO cope with secured mosquitto
        local_config.set_manual_acks(true);
        local_config.set_last_will(LastWill::new(
            &health_topic,
            MQTT_BRIDGE_DOWN_PAYLOAD,
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
            "local",
        ));
        tokio::spawn(half_bridge(
            cloud_event_loop,
            local_client,
            move |topic| format!("{prefix}/{topic}").into(),
            msgs_cloud,
            Some(health_topic),
            "cloud",
        ));

        Self {}
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

/// Forward messages received from `recv_event_loop` to `target`
///
/// The result of running this function constitutes half the MQTT bridge, hence the name.
/// Each half has two main responsibilities, one is to take messages received on the event loop and
/// forward them to the target client, the other is to communicate with the companion half of the
/// bridge to ensure published messages get acknowledged only when they have been fully processed.
///
/// # Message flow
/// Messages in the bridge go through a few states
/// 1. Received from the sending broker
/// 2. Forwarded to the receiving broker
/// 3. The receiving broker sends an acknowledgement
/// 4. The original forwarded message is acknowledged now we know it is fully processed
///
/// Since the function is processing one [EventLoop], messages can be sent to the receiving broker,
/// but we cannot receive acknowledgements from that broker, therefore a communication link must
/// be established between the bridge halves. This link is the argument `companion_bridge_half`,
/// which can both send and receive messages from the other bridge loop.
///
/// When a message is forwarded, the [Publish] is forwarded from this loop to the companion loop.
/// This allows the loop to store the message along with its packet ID when the forwarded message is
/// published. When an acknowledgement is received for the forwarded message, the packet id is used
/// to retrieve the original [Publish], which is then passed to [AsyncClient::ack] to complete the
/// final step of the message flow.
///
/// The channel sends [`Option<Publish>`] rather than [`Publish`] to allow the bridge to send entirely
/// novel messages, and not just forwarded ones, as attaching packet IDs relies on pairing every
/// [Outgoing] publish notification with a message sent by the relevant client. So, when a QoS 1
/// message is forwarded, this will be accompanied by sending `Some(message)` to the channel,
/// allowing the original message to be acknowledged once an acknowledgement is received for the
/// forwarded message. When publishing a health message, this will be accompanied by sending `None`
/// to the channel, telling the bridge to ignore the associated packet ID as this didn't arise from
/// a forwarded message that itself requires acknowledgement.
///
/// ## Bridging local messages to the cloud
///
/// The two `half-bridge` instances cooperate:
///
/// - The `half_bridge(local_event_loop,cloud_client)` receives local messages and publishes these message on the cloud.
/// - The `half_bridge(cloud_event_loop,local_client)` handles the acknowledgements: waiting for messages be acknowledged by the cloud, before sending acks for the original messages.
///
/// ```text
///                   ┌───────────────┐                                   ┌───────────────┐                                           
///                   │  (EventLoop)  │                                   │   (client)    │                                           
///  Incoming::PubAck │     ┌──┐      │                                   │     ┌──┐      │ client.ack                          
///  ─────────────────┼────►│6.├──────┼───────────────────┬───────────────┼────►│7.├──────┼──────────────►                            
///                   │     └──┘      │                   │               │     └──┘      │                                           
///                   │               │                   │               │               │                                           
///                   │               │             ┌─────┼────────┐      │               │                                           
///                   │               │             │     │        │      │               │                                           
/// Outgoing::Publish │     ┌──┐      │             │    ┌┴─┐      │      │               │
///  ─────────────────┼────►│4.├──────┼─────────────┼───►│5.│      │      │               │                                           
///                   │     └─▲┘      │             │    └▲─┘      │      │               │                                           
///                   │       │       │             │     │        │      │               │                                           
///                   │       │       │             │     │        │      │               │                                           
///                   │       │       │             │     │        │      │               │                                           
///                   │       │       │             │     │        │      │               │                                           
///                   │       │       │             │     │        │      │               │ half_bridge(cloud_event_loop,local_client)
///                   │       │       │             │     │        │      │               │                                           
///                   │       │       │             │     │        │      │               │                                           
/// xxxxxxxxxxxxxxxxxxxxxxxxxx│xxxxxxxxxxxxxxxxxxxxx│xxxxx│xxxxxxxx│xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
///                   │       │       │             │     │        │      │               │                                           
///                   │       │       │             │     │        │      │               │                                           
///                   │       │       │             │     │        │      │               │ half_bridge(local_event_loop,cloud_client)
///                   │       │       │             │     │        │      │               │                                           
///                   │       │       │             │     │        │      │               │                                           
///                   │       │       │             │     │        │      │               │                                           
///    client.publish │      ┌┴─┐     │             │    ┌┴─┐      │      │     ┌──┐      │ Incoming::Publish                         
///  ◄────────────────┼──────┤3.◄─────┼─────────────┼────┤2.│◄─────┼──────┼─────┤1.│◄─────┼───────────────                            
///                   │      └──┘     │             │    └──┘      │      │     └──┘      │                                           
///                   │               │             │              │      │               │                                           
///                   │   MQTT        │             │              │      │    MQTT       │                                           
///                   │   cloud       │             └──────────────┘      │    local      │                                           
///                   │   connection  │                                   │    connection │                                           
///                   │               │                                   │               │                                           
///                   │   (client)    │                                   │  (EventLoop)  │                                           
///                   └───────────────┘                                   └───────────────┘                                           
/// ```
///
/// 1. A message is received via the local_event_loop.
/// 2. This message is sent unchanged to the second half_bridge which owns the local_client: so the latter will be able to acknowledge it when fully processed.
/// 3. A copy of this message is published by the cloud_client on the cloud topic derived from the local topic.
/// 4. The cloud_event_loop is notified that the message has been published by the cloud client. The notification event provides the `pkid` of the message used on the cloud connection.
/// 5. The message cloud `pkid` is joined with the original local message sent step 2. The pair (cloud `pkid`, local message) is cached
/// 6. The cloud MQTT end-point acknowledges the message, providing its cloud `pkid`.
/// 7. The pair (cloud `pkid`, local message) is extracted from the cache and the local message is finaly acknowledged.
///  
/// ## Bridging cloud messages to the local broker
///
/// The very same two `half-bridge` instances ensure the reverse flow. Their roles are simply swapped:
///
/// - The `half_bridge(cloud_event_loop,local_client)` receives cloud messages and publishes these message locally.
/// - The `half_bridge(local_event_loop,cloud_client)` handles the acknowledgements: waiting for messages be acknowledged locally, before sending acks for the original messages.
///
/// # Health topics
/// The bridge will publish health information to `health_topic` (if supplied) on `target` to enable
/// other components to establish bridge health. This is intended to be used the half with cloud
/// event loop, so the status of this connection will be relayed to a relevant `te` topic like its
/// mosquitto-based predecessor. The payload is either `1` (healthy) or `0` (unhealthy). When the
/// connection is created, the last-will message is set to send the `0` payload when the connection
/// is dropped.
async fn half_bridge<F: for<'a> Fn(&'a str) -> Cow<'a, str>>(
    mut recv_event_loop: EventLoop,
    target: AsyncClient,
    transform_topic: F,
    mut companion_bridge_half: BidirectionalChannelHalf<Option<Publish>>,
    health_topic: Option<String>,
    name: &'static str,
) {
    let mut forward_pkid_to_received_msg = HashMap::new();
    let mut bridge_health = BridgeHealth::new(name, health_topic, &target);

    loop {
        let res = recv_event_loop.poll().await;
        bridge_health.update(&res, &mut companion_bridge_half).await;

        let notification = match res {
            Ok(notification) => notification,
            Err(_) => continue,
        };

        match notification {
            // Forward messages from event loop to target
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
                let publish = (publish.qos > QoS::AtMostOnce).then_some(publish);
                companion_bridge_half.send(publish).await.unwrap();
            }

            // Forward acks from event loop to target
            Event::Incoming(
                Incoming::PubAck(PubAck { pkid: ack_pkid })
                | Incoming::PubRec(PubRec { pkid: ack_pkid }),
            ) => {
                if let Some(msg) = forward_pkid_to_received_msg.remove(&ack_pkid) {
                    target.ack(&msg).await.unwrap();
                }
            }

            // Keep track of packet IDs so we can acknowledge messages
            Event::Outgoing(Outgoing::Publish(pkid)) => match companion_bridge_half.recv().await {
                // A message was forwarded by the other bridge half, note the packet id
                Some(Some(msg)) => {
                    forward_pkid_to_received_msg.insert(pkid, msg);
                }

                // A healthcheck message was published, ignore this packet id
                Some(None) => {}

                // The other bridge half has disconnected, break the loop and shut down the bridge
                None => break,
            },
            _ => {}
        }
    }
}

type NotificationRes = Result<Event, ConnectionError>;

struct BridgeHealth<'a> {
    name: &'static str,
    health_topic: Option<String>,
    target: &'a AsyncClient,
    last_err: Option<String>,
}

impl<'a> BridgeHealth<'a> {
    fn new(name: &'static str, health_topic: Option<String>, target: &'a AsyncClient) -> Self {
        Self {
            name,
            health_topic,
            target,
            last_err: Some("dummy error".into()),
        }
    }
    async fn update(
        &mut self,
        result: &NotificationRes,
        companion_bridge_half: &mut BidirectionalChannelHalf<Option<Publish>>,
    ) {
        let name = self.name;
        let (err, health_payload) = match result {
            Ok(_) => (None, MQTT_BRIDGE_UP_PAYLOAD),
            Err(err) => (Some(err.to_string()), MQTT_BRIDGE_DOWN_PAYLOAD),
        };

        if self.last_err != err {
            match &err {
                None => info!("MQTT bridge connected to {name} broker"),
                Some(err) => error!("MQTT bridge failed to connect to {name} broker: {err}"),
            }
            self.last_err = err;

            if let Some(health_topic) = &self.health_topic {
                self.target
                    .publish(health_topic, QoS::AtLeastOnce, true, health_payload)
                    .await
                    .unwrap();
                // Send a note that a message has been published to maintain synchronisation
                // between the two bridge halves
                companion_bridge_half.send(None).await.unwrap();
            }
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
        NullSender.into()
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
