#[cfg(feature = "test-helpers")]
pub mod test_helpers;
#[cfg(test)]
mod tests;
pub mod trie;

use async_trait::async_trait;
use mqtt_channel::Connection;
pub use mqtt_channel::DebugPayload;
pub use mqtt_channel::MqttError;
pub use mqtt_channel::MqttMessage;
pub use mqtt_channel::QoS;
use mqtt_channel::SinkExt;
use mqtt_channel::StreamExt;
pub use mqtt_channel::Topic;
pub use mqtt_channel::TopicFilter;
use rumqttc::SubscribeFilter;
use std::convert::Infallible;
use tedge_actors::fan_in_message_type;
use tedge_actors::futures::channel::mpsc;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::ChannelError;
use tedge_actors::ClientMessageBox;
use tedge_actors::DynSender;
use tedge_actors::MessageReceiver;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::Sender;
use tedge_actors::Sequential;
use tedge_actors::Server;
use tedge_actors::ServerActorBuilder;
use tedge_actors::ServerConfig;
use trie::MqtTrie;
use trie::SubscriptionDiff;

pub type MqttConfig = mqtt_channel::Config;

pub struct MqttActorBuilder {
    mqtt_config: mqtt_channel::Config,
    input_receiver: InputCombiner,
    pub_or_sub_sender: PubOrSubSender,
    publish_sender: mpsc::Sender<MqttMessage>,
    subscriber_addresses: Vec<DynMqttSender>,
    signal_sender: mpsc::Sender<RuntimeRequest>,
    trie: TrieService,
    current_id: usize,
    subscription_diff: SubscriptionDiff,
}

struct InputCombiner {
    publish_receiver: mpsc::Receiver<MqttMessage>,
    subscription_request_receiver: mpsc::Receiver<SubscriptionRequest>,
    signal_receiver: mpsc::Receiver<RuntimeRequest>,
}

#[derive(Debug)]
pub enum PublishOrSubscribe {
    Publish(MqttMessage),
    Subscribe(SubscriptionRequest),
}

impl InputCombiner {
    pub fn close_input(&mut self) {
        self.publish_receiver.close();
        self.subscription_request_receiver.close();
        self.signal_receiver.close();
    }
}

#[async_trait]
impl MessageReceiver<PublishOrSubscribe> for InputCombiner {
    async fn try_recv(&mut self) -> Result<Option<PublishOrSubscribe>, RuntimeRequest> {
        tokio::select! {
            biased;

            Some(runtime_request) = self.signal_receiver.next() => {
                Err(runtime_request)
            }
            Some(message) = self.publish_receiver.next() => {
                Ok(Some(PublishOrSubscribe::Publish(message)))
            }
            Some(request) = self.subscription_request_receiver.next() => {
                Ok(Some(PublishOrSubscribe::Subscribe(request)))
            }
            else => Ok(None)
        }
    }

    async fn recv(&mut self) -> Option<PublishOrSubscribe> {
        match self.try_recv().await {
            Ok(Some(message)) => Some(message),
            _ => None,
        }
    }

    async fn recv_signal(&mut self) -> Option<RuntimeRequest> {
        self.signal_receiver.next().await
    }
}

impl MqttActorBuilder {
    pub fn new(config: mqtt_channel::Config) -> Self {
        let (publish_sender, publish_receiver) = mpsc::channel(10);
        let (subscription_request_sender, subscription_request_receiver) = mpsc::channel(10);
        let (signal_sender, signal_receiver) = mpsc::channel(10);
        let trie = TrieService::new(MqtTrie::default());
        let pub_or_sub_sender = PubOrSubSender {
            subscription_request_sender,
            publish_sender: publish_sender.clone(),
        };
        let input_receiver = InputCombiner {
            publish_receiver,
            signal_receiver,
            subscription_request_receiver,
        };

        MqttActorBuilder {
            mqtt_config: config,
            input_receiver,
            publish_sender,
            subscriber_addresses: Vec::new(),
            signal_sender,
            pub_or_sub_sender,
            trie,
            subscription_diff: SubscriptionDiff::empty(),
            current_id: 0,
        }
    }

    pub(crate) fn build_actor(self) -> MqttActor {
        let mut topic_filter = TopicFilter::empty();
        for pattern in &self.subscription_diff.subscribe {
            topic_filter.add(pattern).unwrap();
            tracing::info!(target: "MQTT sub", "{pattern}");
        }
        for pattern in self.subscription_diff.unsubscribe {
            tracing::warn!(target: "MQTT sub", "ignoring overlapping subscription to {pattern}");
        }

        let mqtt_config = self.mqtt_config.with_subscriptions(topic_filter);
        MqttActor::new(
            mqtt_config,
            self.input_receiver,
            self.subscriber_addresses,
            self.trie.builder(),
        )
    }
}

impl AsMut<MqttConfig> for MqttActorBuilder {
    fn as_mut(&mut self) -> &mut MqttConfig {
        &mut self.mqtt_config
    }
}

impl MessageSource<MqttMessage, TopicFilter> for MqttActorBuilder {
    fn connect_sink(&mut self, topics: TopicFilter, peer: &impl MessageSink<MqttMessage>) {
        self.connect_id_sink(topics, peer);
    }
}

impl MqttActorBuilder {
    pub fn connect_id_sink(
        &mut self,
        topics: TopicFilter,
        peer: &impl MessageSink<MqttMessage>,
    ) -> ClientId {
        let sender = DynMqttSender {
            client: ClientId(self.current_id),
            sender: peer.get_sender(),
        };
        let client_id = sender.client;
        self.current_id += 1;
        for topic in topics.patterns() {
            self.subscription_diff += self.trie.trie.insert(topic, client_id);
        }
        self.subscriber_addresses.push(sender);
        client_id
    }
}

impl MessageSink<MqttMessage> for MqttActorBuilder {
    fn get_sender(&self) -> DynSender<MqttMessage> {
        self.publish_sender.clone().into()
    }
}

pub struct TrieService {
    trie: MqtTrie<ClientId>,
}

impl TrieService {
    pub fn new(trie: MqtTrie<ClientId>) -> Self {
        Self { trie }
    }
    pub fn builder(self) -> ServerActorBuilder<Self, Sequential> {
        ServerActorBuilder::new(
            self,
            &ServerConfig::new().with_max_concurrency(1),
            Sequential,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientId(usize);

type MatchRequest = String;

fan_in_message_type!(TrieRequest[SubscriptionRequest, MatchRequest]: Clone, Debug);

#[derive(Debug)]
pub enum TrieResponse {
    Diff(SubscriptionDiff),
    Matched(Vec<ClientId>),
}

#[async_trait]
impl Server for TrieService {
    type Request = TrieRequest;

    type Response = TrieResponse;

    fn name(&self) -> &str {
        "mqtt-subscription-manager"
    }

    async fn handle(&mut self, request: Self::Request) -> Self::Response {
        match request {
            TrieRequest::SubscriptionRequest(req) => {
                let mut diff = SubscriptionDiff::empty();
                for filter in req.diff.subscribe {
                    diff += self.trie.insert(&filter, req.client_id);
                }
                for filter in req.diff.unsubscribe {
                    diff += self.trie.remove(&filter, &req.client_id);
                }
                TrieResponse::Diff(diff)
            }
            TrieRequest::MatchRequest(req) => {
                let res = self.trie.matches(&req);
                TrieResponse::Matched(res.into_iter().cloned().collect())
            }
        }
    }
}

struct DynMqttSender {
    client: ClientId,
    sender: DynSender<MqttMessage>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SubscriptionRequest {
    diff: SubscriptionDiff,
    client_id: ClientId,
}

#[derive(Clone, Debug)]
struct PubOrSubSender {
    publish_sender: mpsc::Sender<MqttMessage>,
    subscription_request_sender: mpsc::Sender<SubscriptionRequest>,
}

#[async_trait]
impl Sender<PublishOrSubscribe> for PubOrSubSender {
    async fn send(&mut self, message: PublishOrSubscribe) -> Result<(), ChannelError> {
        match message {
            PublishOrSubscribe::Publish(msg) => {
                Sender::<_>::send(&mut self.publish_sender, msg).await
            }
            PublishOrSubscribe::Subscribe(sub) => {
                Sender::<_>::send(&mut self.subscription_request_sender, sub).await
            }
        }
    }
}

impl MessageSink<PublishOrSubscribe> for MqttActorBuilder {
    fn get_sender(&self) -> DynSender<PublishOrSubscribe> {
        self.pub_or_sub_sender.clone().into()
    }
}

impl RuntimeRequestSink for MqttActorBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        Box::new(self.signal_sender.clone())
    }
}

impl Builder<MqttActor> for MqttActorBuilder {
    type Error = Infallible;

    fn try_build(self) -> Result<MqttActor, Self::Error> {
        Ok(self.build())
    }

    fn build(self) -> MqttActor {
        self.build_actor()
    }
}

pub struct FromPeers {
    input_receiver: InputCombiner,
    subscriptions: ClientMessageBox<TrieRequest, TrieResponse>,
}

pub struct ToPeers {
    peer_senders: Vec<DynMqttSender>,
    subscriptions: ClientMessageBox<TrieRequest, TrieResponse>,
}

impl FromPeers {
    async fn relay_messages_to(
        &mut self,
        outgoing_mqtt: &mut mpsc::UnboundedSender<MqttMessage>,
        client: mqtt_channel::AsyncClient,
    ) -> Result<(), RuntimeError> {
        while let Ok(Some(message)) = self.try_recv().await {
            match message {
                PublishOrSubscribe::Publish(message) => {
                    tracing::debug!(target: "MQTT pub", "{message}");
                    SinkExt::send(outgoing_mqtt, message)
                        .await
                        .map_err(Box::new)?;
                }
                PublishOrSubscribe::Subscribe(request) => {
                    let TrieResponse::Diff(diff) = self
                        .subscriptions
                        .await_response(TrieRequest::SubscriptionRequest(request))
                        .await
                        .map_err(Box::new)?
                    else {
                        unreachable!("Subscription request always returns diff")
                    };
                    let client = client.clone();
                    tokio::spawn(async move {
                        client
                            .subscribe_many(
                                diff.subscribe
                                    .into_iter()
                                    .map(|path| SubscribeFilter::new(path, QoS::AtLeastOnce)),
                            )
                            .await;
                        for unsub in diff.unsubscribe {
                            client.unsubscribe(unsub).await;
                        }
                    });
                }
            }
        }

        // On shutdown, first close input so no new messages can be pushed
        self.input_receiver.close_input();

        // Then, publish all the messages awaiting to be sent over MQTT
        while let Some(message) = self.recv().await {
            match message {
                PublishOrSubscribe::Publish(message) => {
                    tracing::debug!(target: "MQTT pub", "{message}");
                    SinkExt::send(outgoing_mqtt, message)
                        .await
                        .map_err(Box::new)?;
                }
                // No point creating subscriptions at this point
                PublishOrSubscribe::Subscribe(_) => (),
            }
        }
        Ok(())
    }
}

impl ToPeers {
    async fn relay_messages_from(
        mut self,
        incoming_mqtt: &mut mpsc::UnboundedReceiver<MqttMessage>,
    ) -> Result<(), RuntimeError> {
        while let Some(message) = incoming_mqtt.next().await {
            tracing::debug!(target: "MQTT recv", "{message}");
            self.send(message).await?;
        }
        Ok(())
    }

    async fn send(&mut self, message: MqttMessage) -> Result<(), ChannelError> {
        let subscribed = self
            .subscriptions
            .await_response(TrieRequest::MatchRequest(message.topic.name.clone()))
            .await?;
        let TrieResponse::Matched(matches) = subscribed else {
            unreachable!("MatchRequest always returns Matched")
        };
        for client in matches {
            self.peer_senders[client.0]
                .sender
                .send(message.clone())
                .await?;
        }
        Ok(())
    }
}

#[async_trait]
impl MessageReceiver<PublishOrSubscribe> for FromPeers {
    async fn try_recv(&mut self) -> Result<Option<PublishOrSubscribe>, RuntimeRequest> {
        self.input_receiver.try_recv().await
    }

    async fn recv(&mut self) -> Option<PublishOrSubscribe> {
        self.input_receiver.recv().await
    }

    async fn recv_signal(&mut self) -> Option<RuntimeRequest> {
        self.input_receiver.recv_signal().await
    }
}

pub struct MqttActor {
    mqtt_config: mqtt_channel::Config,
    from_peers: FromPeers,
    to_peers: ToPeers,
    trie_service: ServerActorBuilder<TrieService, Sequential>,
}

impl MqttActor {
    fn new(
        mqtt_config: mqtt_channel::Config,
        input_receiver: InputCombiner,
        peer_senders: Vec<DynMqttSender>,
        mut trie_service: ServerActorBuilder<TrieService, Sequential>,
    ) -> Self {
        MqttActor {
            mqtt_config,
            from_peers: FromPeers {
                input_receiver,
                subscriptions: ClientMessageBox::new(&mut trie_service),
            },
            to_peers: ToPeers {
                peer_senders,
                subscriptions: ClientMessageBox::new(&mut trie_service),
            },
            trie_service,
        }
    }
}

#[async_trait]
impl Actor for MqttActor {
    fn name(&self) -> &str {
        "MQTT"
    }

    async fn run(mut self) -> Result<(), RuntimeError> {
        let mut mqtt_client = tokio::select! {
            connection = mqtt_channel::Connection::new(&self.mqtt_config) => {
                connection.map_err(Box::new)?
            }
            Some(RuntimeRequest::Shutdown) = self.from_peers.recv_signal() => {
                // Shutdown requested even before the connection has been established
                return Ok(())
            }
        };

        tokio::spawn(async move { self.trie_service.run().await });

        tedge_utils::futures::select(
            self.from_peers
                .relay_messages_to(&mut mqtt_client.published, mqtt_client.client.clone()),
            self.to_peers.relay_messages_from(&mut mqtt_client.received),
        )
        .await
    }
}

#[async_trait]
pub trait MqttConnector: Send {
    async fn connect(&mut self, topics: TopicFilter) -> Result<Box<dyn MqttConnection>, MqttError>;
}

#[async_trait]
pub trait MqttConnection: Send {
    async fn next_message(&mut self) -> Option<MqttMessage>;

    async fn disconnect(self: Box<Self>);
}

pub struct MqttConnectionImpl {
    connection: Connection,
}

impl MqttConnectionImpl {
    fn new(connection: Connection) -> Self {
        Self { connection }
    }
}

#[async_trait]
impl MqttConnection for MqttConnectionImpl {
    async fn next_message(&mut self) -> Option<MqttMessage> {
        self.connection.received.next().await
    }

    async fn disconnect(self: Box<Self>) {
        self.connection.close().await;
    }
}

pub struct MqttDynamicConnector {
    base_mqtt_config: MqttConfig,
}

impl MqttDynamicConnector {
    pub fn new(base_mqtt_config: MqttConfig) -> Self {
        Self { base_mqtt_config }
    }
}

#[async_trait]
impl MqttConnector for MqttDynamicConnector {
    async fn connect(&mut self, topics: TopicFilter) -> Result<Box<dyn MqttConnection>, MqttError> {
        let mqtt_config = self.base_mqtt_config.clone().with_subscriptions(topics);
        let connection = mqtt_channel::Connection::new(&mqtt_config).await?;
        Ok(Box::new(MqttConnectionImpl::new(connection)))
    }
}
