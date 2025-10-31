#[cfg(feature = "test-helpers")]
pub mod test_helpers;
#[cfg(test)]
mod tests;
pub mod trie;

use async_trait::async_trait;
pub use mqtt_channel::DebugPayload;
pub use mqtt_channel::MqttError;
pub use mqtt_channel::MqttMessage;
pub use mqtt_channel::QoS;
use mqtt_channel::SinkExt;
use mqtt_channel::StreamExt;
use mqtt_channel::SubscriberOps;
pub use mqtt_channel::Topic;
pub use mqtt_channel::TopicFilter;
use std::collections::HashSet;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;
use tedge_actors::fan_in_message_type;
use tedge_actors::futures::channel::mpsc;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::ChannelError;
use tedge_actors::ClientMessageBox;
use tedge_actors::CloneSender;
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
use trie::RankTopicFilter;
pub use trie::SubscriptionDiff;

pub type MqttConfig = mqtt_channel::Config;

pub struct MqttActorBuilder {
    mqtt_config: mqtt_channel::Config,
    input_receiver: InputCombiner,
    request_sender: mpsc::Sender<MqttRequest>,
    subscriber_addresses: Vec<DynSender<MqttMessage>>,
    signal_sender: mpsc::Sender<RuntimeRequest>,
    trie: TrieService,
    current_id: usize,
    subscription_diff: SubscriptionDiff,
    dynamic_connect_sender: mpsc::Sender<(
        TrieInsertRequest,
        Box<dyn CloneSender<MqttMessage> + 'static>,
    )>,
    dynamic_connect_receiver: mpsc::Receiver<(
        TrieInsertRequest,
        Box<dyn CloneSender<MqttMessage> + 'static>,
    )>,
}

impl MqttRequest {
    pub fn subscribe(client_id: ClientId, diff: SubscriptionDiff) -> Self {
        MqttRequest::Subscribe(SubscriptionRequest { diff, client_id })
    }
}

struct InputCombiner {
    signal_receiver: mpsc::Receiver<RuntimeRequest>,
    request_receiver: mpsc::Receiver<MqttRequest>,
}

impl MessageSource<MqttMessage, &mut DynSubscriptions> for MqttActorBuilder {
    fn connect_sink(
        &mut self,
        subscriptions: &mut DynSubscriptions,
        peer: &impl MessageSink<MqttMessage>,
    ) {
        let client_id = self.connect_id_sink(subscriptions.init_topics.clone(), peer);
        subscriptions.client_id = Some(client_id);
    }
}

/// A handle to retrieve your [ClientId] when connecting a sink to the MQTT
/// actor
///
/// This [ClientId] can later be used to update the subscriptions of an existing
/// MQTT connection once the actors are running.
///
/// ```
/// use tedge_actors::*;
/// use tedge_mqtt_ext::{DynSubscriptions, ClientId, MqttMessage, MqttRequest, TopicFilter};
///
/// struct MyActorBuilder {
///     msgs: SimpleMessageBoxBuilder<MqttMessage, MqttRequest>,
///     // Client ID is used to send [MqttRequest::Subscribe] requests once the actor is running
///     client_id: ClientId,
/// }
///
/// impl MyActorBuilder {
///     fn new<M>(mut mqtt: M) -> Self
///     where
///         M: for<'a> MessageSource<MqttMessage, &'a mut DynSubscriptions>,
///     {
///         let mut subs = DynSubscriptions::new(TopicFilter::empty());
///         let msgs = SimpleMessageBoxBuilder::new("MyActor", 16);
///         mqtt.connect_sink(&mut subs, &msgs);
///
///         Self {
///             msgs,
///             client_id: subs.client_id(),
///         }
///     }
/// }
/// ```
pub struct DynSubscriptions {
    init_topics: TopicFilter,
    client_id: Option<ClientId>,
}

impl DynSubscriptions {
    pub fn new(init_topics: TopicFilter) -> Self {
        DynSubscriptions {
            init_topics,
            client_id: None,
        }
    }

    /// Retrieves the client ID
    ///
    /// This is a consuming method since the client ID can only be retrieved
    /// after connecting to the MQTT actor
    pub fn client_id(self) -> ClientId {
        self.client_id.unwrap()
    }

    #[cfg(feature = "test-helpers")]
    pub fn set_client_id_usize(&mut self, value: usize) {
        self.client_id = Some(ClientId(value))
    }
}

#[cfg(feature = "test-helpers")]
impl TryFrom<MqttRequest> for MqttMessage {
    type Error = anyhow::Error;
    fn try_from(value: MqttRequest) -> Result<Self, Self::Error> {
        if let MqttRequest::Publish(msg) = value {
            Ok(msg)
        } else {
            Err(anyhow::anyhow!("{value:?} is not an MQTT message!"))
        }
    }
}

#[derive(Debug)]
pub enum MqttRequest {
    Publish(MqttMessage),
    Subscribe(SubscriptionRequest),
    /// A one-shot request for all the retain messages for a set of topics
    ///
    /// The provided sender is used to send those retained messages back to the
    /// requesting peer
    RetrieveRetain(mpsc::UnboundedSender<MqttMessage>, TopicFilter),
}

impl PartialEq<MqttMessage> for MqttRequest {
    fn eq(&self, other: &MqttMessage) -> bool {
        if let Self::Publish(message) = self {
            message == other
        } else {
            false
        }
    }
}

impl From<MqttMessage> for MqttRequest {
    fn from(message: MqttMessage) -> Self {
        Self::Publish(message)
    }
}

#[derive(Clone)]
pub struct DynamicMqttClientHandle {
    current_id: Arc<tokio::sync::Mutex<usize>>,
    tx: mpsc::Sender<(
        TrieInsertRequest,
        Box<dyn CloneSender<MqttMessage> + 'static>,
    )>,
}

impl DynamicMqttClientHandle {
    pub async fn connect_sink_dynamic(
        &mut self,
        topics: TopicFilter,
        peer: &impl MessageSink<MqttMessage>,
    ) -> ClientId {
        let mut current_id = self.current_id.lock().await;
        let client_id = ClientId(*current_id);
        self.tx
            .send((TrieInsertRequest { client_id, topics }, peer.get_sender()))
            .await
            .unwrap();
        *current_id += 1;
        client_id
    }
}

impl InputCombiner {
    pub fn close_input(&mut self) {
        self.request_receiver.close();
        self.signal_receiver.close();
    }
}

#[async_trait]
impl MessageReceiver<MqttRequest> for InputCombiner {
    async fn try_recv(&mut self) -> Result<Option<MqttRequest>, RuntimeRequest> {
        tokio::select! {
            biased;

            Some(runtime_request) = self.signal_receiver.next() => {
                Err(runtime_request)
            }
            Some(request) = self.request_receiver.next() => {
                Ok(Some(request))
            }
            else => Ok(None)
        }
    }

    async fn recv(&mut self) -> Option<MqttRequest> {
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
        let (request_sender, request_receiver) = mpsc::channel(10);
        let (signal_sender, signal_receiver) = mpsc::channel(10);
        let (dynamic_connect_sender, dynamic_connect_receiver) = mpsc::channel(10);
        let trie = TrieService::new(MqtTrie::default());
        let input_receiver = InputCombiner {
            signal_receiver,
            request_receiver,
        };

        MqttActorBuilder {
            mqtt_config: config,
            input_receiver,
            subscriber_addresses: Vec::new(),
            signal_sender,
            request_sender,
            trie,
            subscription_diff: SubscriptionDiff::empty(),
            current_id: 0,
            dynamic_connect_sender,
            dynamic_connect_receiver,
        }
    }

    pub(crate) fn build_actor(self) -> MqttActor {
        let mut topic_filter = TopicFilter::empty();
        for pattern in &self.subscription_diff.subscribe {
            topic_filter.try_add(pattern).unwrap();
            tracing::info!(target: "MQTT sub", "{pattern}");
        }

        let base_config = self
            .mqtt_config
            .clone()
            .with_no_session()
            .with_no_last_will_or_initial_message();
        let mqtt_config = self.mqtt_config.with_subscriptions(topic_filter);
        MqttActor::new(
            mqtt_config,
            base_config,
            self.input_receiver,
            self.subscriber_addresses,
            self.trie.builder(),
            DynamicMqttClientHandle {
                current_id: Arc::new(tokio::sync::Mutex::new(self.current_id)),
                tx: self.dynamic_connect_sender,
            },
            self.dynamic_connect_receiver,
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
        let client_id = self.add_new_subscriber(peer.get_sender());
        for topic in topics.patterns() {
            self.subscription_diff += self.trie.trie.insert(topic, client_id);
        }
        client_id
    }

    fn add_new_subscriber(
        &mut self,
        sender: Box<dyn CloneSender<MqttMessage> + 'static>,
    ) -> ClientId {
        let client_id = ClientId(self.current_id);
        self.subscriber_addresses.push(sender);
        self.current_id += 1;
        client_id
    }
}

impl MessageSink<MqttMessage> for MqttActorBuilder {
    fn get_sender(&self) -> DynSender<MqttMessage> {
        Box::new(self.request_sender.clone())
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

#[cfg(feature = "test-helpers")]
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct ClientId(pub usize);
#[cfg(not(feature = "test-helpers"))]
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct ClientId(usize);

type MatchRequest = String;

struct TrieInsertRequest {
    client_id: ClientId,
    topics: TopicFilter,
}

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

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SubscriptionRequest {
    diff: SubscriptionDiff,
    client_id: ClientId,
}

impl MessageSink<MqttRequest> for MqttActorBuilder {
    fn get_sender(&self) -> DynSender<MqttRequest> {
        self.request_sender.clone().into()
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
    base_config: mqtt_channel::Config,
    subscriptions: ClientMessageBox<TrieRequest, TrieResponse>,
}

pub struct ToPeers {
    peer_senders: Vec<DynSender<MqttMessage>>,
    subscriptions: ClientMessageBox<TrieRequest, TrieResponse>,
}

impl FromPeers {
    async fn try_recv(
        &mut self,
        rx_to_peers: &mut mpsc::UnboundedReceiver<MqttRequest>,
    ) -> Result<Option<MqttRequest>, RuntimeRequest> {
        tokio::select! {
            msg = self.input_receiver.try_recv() => msg,
            msg = rx_to_peers.next() => Ok(msg),
        }
    }

    async fn relay_messages_to(
        &mut self,
        outgoing_mqtt: &mut mpsc::UnboundedSender<MqttMessage>,
        tx_to_peers: &mut mpsc::UnboundedSender<(ClientId, MqttMessage)>,
        client: impl SubscriberOps + Clone + Send + 'static,
        rx_to_peers: &mut mpsc::UnboundedReceiver<MqttRequest>,
    ) -> Result<(), RuntimeError> {
        while let Ok(Some(message)) = self.try_recv(rx_to_peers).await {
            match message {
                MqttRequest::Publish(message) => {
                    tracing::debug!(target: "MQTT pub", "{message}");
                    SinkExt::send(outgoing_mqtt, message)
                        .await
                        .map_err(Box::new)?;
                }
                MqttRequest::Subscribe(request) => {
                    let TrieResponse::Diff(diff) = self
                        .subscriptions
                        .await_response(TrieRequest::SubscriptionRequest(request.clone()))
                        .await
                        .map_err(Box::new)?
                    else {
                        unreachable!("Subscription request always returns diff")
                    };
                    let overlapping_subscriptions = request
                        .diff
                        .subscribe
                        .iter()
                        .filter(|s| {
                            !diff
                                .subscribe
                                .iter()
                                .any(|s2| RankTopicFilter(s2) >= RankTopicFilter(s))
                        })
                        .collect::<Vec<_>>();
                    let client = client.clone();
                    tokio::spawn(async move {
                        // We're running outside the main task, so we can't return an error
                        // In practice, this should never fail
                        if !diff.subscribe.is_empty() {
                            client.subscribe_many(diff.subscribe).await.unwrap();
                        }
                        if !diff.unsubscribe.is_empty() {
                            client.unsubscribe_many(diff.unsubscribe).await.unwrap();
                        }
                    });
                    let mut tf = TopicFilter::empty();
                    for sub in overlapping_subscriptions {
                        tf.add_unchecked(sub);
                    }
                    if !tf.is_empty() {
                        self.forward_retain_messages_to(tx_to_peers.clone(), tf, move |msg| {
                            (request.client_id, msg)
                        });
                    }
                }
                MqttRequest::RetrieveRetain(tx, topics) => {
                    // We don't need to create a long-lived subscription, just
                    // forward the retain messages for these topics
                    self.forward_retain_messages_to(tx, topics, move |msg| msg);
                }
            }
        }

        // On shutdown, first close input so no new messages can be pushed
        self.input_receiver.close_input();

        // Then, publish all the messages awaiting to be sent over MQTT
        while let Some(message) = self.recv().await {
            match message {
                MqttRequest::Publish(message) => {
                    tracing::debug!(target: "MQTT pub", "{message}");
                    SinkExt::send(outgoing_mqtt, message)
                        .await
                        .map_err(Box::new)?;
                }
                // No point creating subscriptions at this point
                MqttRequest::Subscribe(_) => (),
                MqttRequest::RetrieveRetain(_, _) => (),
            }
        }
        Ok(())
    }

    fn forward_retain_messages_to<Packet: Send + 'static>(
        &self,
        mut sender: mpsc::UnboundedSender<Packet>,
        topics: TopicFilter,
        prepare_msg: impl (Fn(MqttMessage) -> Packet) + Send + 'static,
    ) {
        let dynamic_connection_config = self.base_config.clone().with_subscriptions(topics);
        tokio::spawn(async move {
            let mut conn = mqtt_channel::Connection::new(&dynamic_connection_config)
                .await
                .unwrap();
            let mut last_retain_message = Instant::now();
            while let Ok(msg) =
                tokio::time::timeout(Duration::from_secs(1), conn.received.next()).await
            {
                if let Some(msg) = msg {
                    if msg.retain {
                        SinkExt::send(&mut sender, prepare_msg(msg)).await.unwrap();
                        last_retain_message = Instant::now();
                    }
                } else {
                    break;
                }
                // Ensure we break out of the loop even if one of the topics
                // sees a lot of non-retain messages
                if last_retain_message.elapsed() > Duration::from_secs(1) {
                    break;
                }
            }
            conn.close().await;
        });
    }
}

impl ToPeers {
    async fn relay_messages_from(
        mut self,
        incoming_mqtt: &mut mpsc::UnboundedReceiver<MqttMessage>,
        rx_from_peers: &mut mpsc::UnboundedReceiver<(ClientId, MqttMessage)>,
        dynamic_connection_request: &mut mpsc::Receiver<(
            TrieInsertRequest,
            Box<dyn CloneSender<MqttMessage> + 'static>,
        )>,
        tx_from_peers: &mut mpsc::UnboundedSender<MqttRequest>,
    ) -> Result<(), RuntimeError> {
        loop {
            tokio::select! {
                message = incoming_mqtt.next() => {
                    let Some(message) = message else { break };
                    tracing::debug!(target: "MQTT recv", "{message}");
                    self.send(message).await?;
                }
                message = rx_from_peers.next() => {
                    let Some((client, message)) = message else { break };
                    tracing::debug!(target: "MQTT recv", "{message}");
                    self.sender_by_id(client).send(message.clone()).await?;
                }
                Some((insert_req, sender)) = dynamic_connection_request.next() => {
                    self.peer_senders.push(sender);
                    SinkExt::send(
                        tx_from_peers,
                        MqttRequest::Subscribe(SubscriptionRequest {
                            diff: SubscriptionDiff {
                                subscribe: insert_req.topics.patterns().iter().cloned().collect(),
                                unsubscribe: <_>::default(),
                            },
                            client_id: insert_req.client_id,
                        }),
                    )
                    .await?;
                }
            };
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
        for client in HashSet::<ClientId>::from_iter(matches) {
            self.sender_by_id(client).send(message.clone()).await?;
        }
        Ok(())
    }

    fn sender_by_id(&mut self, id: ClientId) -> &mut Box<dyn CloneSender<MqttMessage>> {
        &mut self.peer_senders[id.0]
    }
}

#[async_trait]
impl MessageReceiver<MqttRequest> for FromPeers {
    async fn try_recv(&mut self) -> Result<Option<MqttRequest>, RuntimeRequest> {
        self.input_receiver.try_recv().await
    }

    async fn recv(&mut self) -> Option<MqttRequest> {
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
    dynamic_client_handle: DynamicMqttClientHandle,
    dynamic_connect_receiver: mpsc::Receiver<(
        TrieInsertRequest,
        Box<dyn CloneSender<MqttMessage> + 'static>,
    )>,
}

impl MqttActor {
    fn new(
        mqtt_config: mqtt_channel::Config,
        base_config: mqtt_channel::Config,
        input_receiver: InputCombiner,
        peer_senders: Vec<DynSender<MqttMessage>>,
        mut trie_service: ServerActorBuilder<TrieService, Sequential>,
        dynamic_client_handle: DynamicMqttClientHandle,
        dynamic_connect_receiver: mpsc::Receiver<(
            TrieInsertRequest,
            Box<dyn CloneSender<MqttMessage> + 'static>,
        )>,
    ) -> Self {
        MqttActor {
            mqtt_config,
            from_peers: FromPeers {
                input_receiver,
                base_config,
                subscriptions: ClientMessageBox::new(&mut trie_service),
            },
            to_peers: ToPeers {
                peer_senders,
                subscriptions: ClientMessageBox::new(&mut trie_service),
            },
            trie_service,
            dynamic_client_handle,
            dynamic_connect_receiver,
        }
    }

    pub fn dynamic_client_handle(&self) -> DynamicMqttClientHandle {
        self.dynamic_client_handle.clone()
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
        let (mut to_peer, mut from_peer) = mpsc::unbounded();
        let (mut to_from_peer, mut from_to_peer) = mpsc::unbounded();

        tokio::spawn(async move { self.trie_service.run().await });

        tedge_utils::futures::select(
            self.from_peers.relay_messages_to(
                &mut mqtt_client.published,
                &mut to_peer,
                mqtt_client.subscriptions,
                &mut from_to_peer,
            ),
            self.to_peers.relay_messages_from(
                &mut mqtt_client.received,
                &mut from_peer,
                &mut self.dynamic_connect_receiver,
                &mut to_from_peer,
            ),
        )
        .await
    }
}

#[cfg(test)]
mod unit_tests {
    use std::collections::HashMap;
    use std::collections::VecDeque;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::time::Duration;

    use super::*;
    use async_trait::async_trait;

    macro_rules! subscription_request {
        (sub: $($sub:literal),*; unsub: $($unsub:literal),*; id: $id:literal $(;)?) => {
            SubscriptionRequest {
                diff: SubscriptionDiff {
                    subscribe: [$($sub.into()),*].into(),
                    unsubscribe: [$($unsub.into()),*].into(),
                },
                client_id: ClientId($id),
            }
        };
    }

    #[tokio::test]
    async fn subscribes_upon_receiving_subscribe_request() {
        let mut actor = MqttActorTest::new(&[]);

        actor
            .send_sub(subscription_request!(
                sub: "a/b";
                unsub: ;
                id: 0
            ))
            .await;

        actor
            .subscribe_client
            .assert_subscribed_to(["a/b".into()])
            .await;

        actor.close().await;
    }

    #[tokio::test]
    async fn unsubscribes_upon_receiving_unsubscribe_request() {
        let mut actor = MqttActorTest::new(&[("a/b", 0)]);

        actor
            .send_sub(subscription_request!(
                sub: ;
                unsub: "a/b";
                id: 0
            ))
            .await;

        actor
            .subscribe_client
            .assert_unsubscribed_from(["a/b".into()])
            .await;

        actor.close().await;
    }

    #[tokio::test]
    async fn only_subscribes_to_a_minimal_set_of_topics() {
        let mut actor = MqttActorTest::new(&[]);

        actor
            .send_sub(subscription_request!(
                sub: "a/+", "#";
                unsub: ;
                id: 0
            ))
            .await;

        actor
            .subscribe_client
            .assert_subscribed_to(["#".into()])
            .await;

        actor.close().await;
    }

    #[tokio::test]
    async fn published_messages_are_forwarded_to_mqtt_channel() {
        let mut actor = MqttActorTest::new(&[]);

        actor.publish("a/b", "test message").await;

        let message = tokio::time::timeout(Duration::from_secs(5), actor.sent_to_channel.next())
            .await
            .unwrap();
        assert_eq!(
            message,
            Some(MqttMessage::new(
                &Topic::new("a/b").unwrap(),
                "test message"
            ))
        );

        actor.close().await;
    }

    #[tokio::test]
    async fn publishes_messages_only_to_subscribed_clients() {
        let mut actor = MqttActorTest::new(&[("a/b", 0), ("b/c", 1)]);

        actor.receive("b/c", "test message").await;

        assert_eq!(
            actor.next_message_for(1).await,
            MqttMessage::new(&Topic::new("b/c").unwrap(), "test message")
        );
        assert!(actor
            .sent_to_clients
            .get_mut(&0)
            .unwrap()
            .try_next()
            .is_err());

        actor.close().await;
    }

    #[tokio::test]
    async fn publishes_messages_to_dynamically_subscribed_clients() {
        let mut actor = MqttActorTest::new(&[]);

        let client_id = actor
            .connect_dynamic(TopicFilter::new_unchecked("b/c"))
            .await;

        actor
            .subscribe_client
            .assert_subscribed_to(["b/c".into()])
            .await;

        actor.receive("b/c", "test message").await;

        assert_eq!(
            actor.next_message_for(client_id).await,
            MqttMessage::new(&Topic::new("b/c").unwrap(), "test message")
        );

        actor.close().await;
    }

    #[tokio::test]
    async fn copes_with_dynamic_connection_handle_closing() {
        // This shouldn't ever happen in practice, but the initial
        // implementation went into a hard loop if the channel closed, so if
        // this situation were to arise, it would have been catastrophic
        let mut actor = MqttActorTest::new(&[]);

        let client_id = actor
            .connect_dynamic(TopicFilter::new_unchecked("b/c"))
            .await;

        // As explained above, this shouldn't possible with the public API,
        // hence digging into the `DynamicMqttClientHandle` itself
        actor.dyn_connect.tx.close_channel();

        actor
            .subscribe_client
            .assert_subscribed_to(["b/c".into()])
            .await;

        actor.receive("b/c", "test message").await;

        assert_eq!(
            actor.next_message_for(client_id).await,
            MqttMessage::new(&Topic::new("b/c").unwrap(), "test message")
        );

        actor.close().await;
    }

    #[tokio::test]
    async fn publishes_messages_only_to_subscribed_dynamic_client() {
        let mut actor = MqttActorTest::new(&[]);

        let client_id = actor
            .connect_dynamic(TopicFilter::new_unchecked("a/b"))
            .await;
        let client_id_2 = actor
            .connect_dynamic(TopicFilter::new_unchecked("b/c"))
            .await;

        actor
            .subscribe_client
            .assert_subscribed_to(["a/b".into()])
            .await;
        actor
            .subscribe_client
            .assert_subscribed_to(["b/c".into()])
            .await;

        actor.receive("b/c", "test message").await;

        assert_eq!(
            actor.next_message_for(client_id_2).await,
            MqttMessage::new(&Topic::new("b/c").unwrap(), "test message")
        );
        assert!(actor
            .sent_to_clients
            .get_mut(&client_id)
            .unwrap()
            .try_next()
            .is_err());

        actor.close().await;
    }

    #[tokio::test]
    async fn publishes_messages_separately_to_dynamic_and_non_dynamic_clients() {
        let mut actor = MqttActorTest::new(&[("a/b", 0)]);

        let static_id = 0;
        let dynamic_id = actor
            .connect_dynamic(TopicFilter::new_unchecked("b/c"))
            .await;

        actor
            .subscribe_client
            .assert_subscribed_to(["b/c".into()])
            .await;

        actor.receive("a/b", "test message").await;
        actor.receive("b/c", "test message").await;

        assert_eq!(
            actor.next_message_for(static_id).await,
            MqttMessage::new(&Topic::new("a/b").unwrap(), "test message")
        );
        assert_eq!(
            actor.next_message_for(dynamic_id).await,
            MqttMessage::new(&Topic::new("b/c").unwrap(), "test message")
        );

        actor.close().await;
    }

    struct MqttActorTest {
        subscribe_client: MockSubscriberOps,
        req_tx: mpsc::Sender<MqttRequest>,
        sent_to_channel: mpsc::UnboundedReceiver<MqttMessage>,
        sent_to_clients: HashMap<usize, mpsc::Receiver<MqttMessage>>,
        inject_received_message: mpsc::UnboundedSender<MqttMessage>,
        from_peers: Option<tokio::task::JoinHandle<Result<(), RuntimeError>>>,
        to_peers: Option<tokio::task::JoinHandle<Result<(), RuntimeError>>>,
        waited: bool,
        dyn_connect: DynamicMqttClientHandle,
    }

    impl Drop for MqttActorTest {
        fn drop(&mut self) {
            if !std::thread::panicking() && !self.waited {
                panic!("Call `MqttActorTest::close` at the end of the test")
            }
        }
    }

    impl MqttActorTest {
        pub fn new(default_subscriptions: &[(&str, usize)]) -> Self {
            let (req_tx, req_rx) = mpsc::channel(10);
            let (_sig_tx, sig_rx) = mpsc::channel(10);
            let (mut outgoing_mqtt, sent_messages) = mpsc::unbounded();
            let (inject_received_message, mut incoming_messages) = mpsc::unbounded();
            let input_combiner = InputCombiner {
                signal_receiver: sig_rx,
                request_receiver: req_rx,
            };

            let mut ts = TrieService::with_default_subscriptions(default_subscriptions);
            let mut fp = FromPeers {
                input_receiver: input_combiner,
                base_config: <_>::default(),
                subscriptions: ClientMessageBox::new(&mut ts),
            };
            let mut sent_to_clients = HashMap::new();
            let mut peer_senders = Vec::new();
            let max_client_id = default_subscriptions.iter().map(|(_, id)| id).max();
            if let Some(&max_id) = max_client_id {
                for id in 0..=max_id {
                    let (tx, rx) = mpsc::channel(10);
                    peer_senders.push(Box::new(tx) as DynSender<_>);
                    sent_to_clients.insert(id, rx);
                }
            }
            let tp = ToPeers {
                subscriptions: ClientMessageBox::new(&mut ts),
                peer_senders,
            };
            tokio::spawn(async move { ts.build().run().await });

            let (mut tx, mut rx) = mpsc::unbounded();
            let (mut tx2, mut rx2) = mpsc::unbounded();
            let (dyn_connect_tx, mut dyn_connect_rx) = mpsc::channel(10);

            let subscribe_client = MockSubscriberOps::default();
            let from_peers = {
                let client = subscribe_client.clone();
                tokio::spawn(async move {
                    fp.relay_messages_to(&mut outgoing_mqtt, &mut tx, client, &mut rx2)
                        .await
                })
            };
            let to_peers = tokio::spawn(async move {
                tp.relay_messages_from(
                    &mut incoming_messages,
                    &mut rx,
                    &mut dyn_connect_rx,
                    &mut tx2,
                )
                .await
            });

            Self {
                subscribe_client,
                req_tx,
                sent_to_clients,
                sent_to_channel: sent_messages,
                inject_received_message,
                from_peers: Some(from_peers),
                to_peers: Some(to_peers),
                waited: false,
                dyn_connect: DynamicMqttClientHandle {
                    current_id: Arc::new(tokio::sync::Mutex::new(
                        max_client_id.map_or(0, |&max| max + 1),
                    )),
                    tx: dyn_connect_tx,
                },
            }
        }

        pub async fn connect_dynamic(&mut self, topics: TopicFilter) -> usize {
            struct ChannelSink(mpsc::Sender<MqttMessage>);

            impl MessageSink<MqttMessage> for ChannelSink {
                fn get_sender(&self) -> DynSender<MqttMessage> {
                    Box::new(self.0.clone())
                }
            }

            let (tx, rx) = mpsc::channel(10);
            let dyn_client_id = self
                .dyn_connect
                .connect_sink_dynamic(topics, &ChannelSink(tx))
                .await;
            self.sent_to_clients.insert(dyn_client_id.0, rx);
            dyn_client_id.0
        }

        /// Closes the channels associated with this actor and waits for both
        /// loops to finish executing
        ///
        /// This allows the `SubscriberOps::drop` implementation to reliably
        /// flag any unasserted communication
        pub async fn close(mut self) {
            self.req_tx.close_channel();
            self.inject_received_message.close_channel();
            self.from_peers.take().unwrap().await.unwrap().unwrap();
            self.to_peers.take().unwrap().await.unwrap().unwrap();
            self.waited = true;
        }

        /// Simulates a client sending a subscription request to the mqtt actor
        pub async fn send_sub(&mut self, req: SubscriptionRequest) {
            SinkExt::send(&mut self.req_tx, MqttRequest::Subscribe(req))
                .await
                .unwrap();
        }

        /// Simulates a client sending a publish request to the mqtt actor
        pub async fn publish(&mut self, topic: &str, payload: &str) {
            SinkExt::send(
                &mut self.req_tx,
                MqttRequest::Publish(MqttMessage::new(&Topic::new(topic).unwrap(), payload)),
            )
            .await
            .unwrap();
        }

        /// Simulates receiving a message from the mqtt channel
        pub async fn receive(&mut self, topic: &str, payload: &str) {
            SinkExt::send(
                &mut self.inject_received_message,
                MqttMessage::new(&Topic::new(topic).unwrap(), payload),
            )
            .await
            .unwrap();
        }

        pub async fn next_message_for(&mut self, id: usize) -> MqttMessage {
            let fut = self.sent_to_clients.get_mut(&id).unwrap().next();
            tokio::time::timeout(Duration::from_secs(5), fut)
                .await
                .unwrap()
                .unwrap()
        }
    }

    impl TrieService {
        fn with_default_subscriptions(
            default_subscriptions: &[(&str, usize)],
        ) -> ServerActorBuilder<TrieService, Sequential> {
            let mut trie = MqtTrie::default();
            for (sub, id) in default_subscriptions {
                trie.insert(sub, ClientId(*id));
            }
            TrieService::new(trie).builder()
        }
    }

    #[derive(Clone, Debug, Default)]
    struct MockSubscriberOps {
        subscribe_many: Arc<Mutex<VecDeque<Vec<String>>>>,
        unsubscribe_many: Arc<Mutex<VecDeque<Vec<String>>>>,
    }

    #[async_trait]
    impl SubscriberOps for MockSubscriberOps {
        async fn subscribe_many(
            &self,
            topics: impl IntoIterator<Item = String> + Send,
        ) -> Result<(), MqttError> {
            self.subscribe_many
                .lock()
                .unwrap()
                .push_back(topics.into_iter().collect());
            Ok(())
        }
        async fn unsubscribe_many(
            &self,
            topics: impl IntoIterator<Item = String> + Send,
        ) -> Result<(), MqttError> {
            self.unsubscribe_many
                .lock()
                .unwrap()
                .push_back(topics.into_iter().collect());
            Ok(())
        }
    }

    impl MockSubscriberOps {
        async fn assert_subscribed_to(&self, filters: impl IntoIterator<Item = String> + Send) {
            let called_with = tokio::time::timeout(Duration::from_secs(5), async move {
                loop {
                    if let Some(topic) = self.subscribe_many.lock().unwrap().pop_front() {
                        break topic;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            assert_eq!(called_with, filters.into_iter().collect::<Vec<_>>())
        }

        async fn assert_unsubscribed_from(&self, filters: impl IntoIterator<Item = String> + Send) {
            let called_with = tokio::time::timeout(Duration::from_secs(5), async move {
                loop {
                    if let Some(topic) = self.unsubscribe_many.lock().unwrap().pop_front() {
                        break topic;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            assert_eq!(called_with, filters.into_iter().collect::<Vec<_>>())
        }
    }

    impl Drop for MockSubscriberOps {
        fn drop(&mut self) {
            if std::thread::panicking() {
                return;
            }
            if Arc::strong_count(&self.subscribe_many) > 1 {
                return;
            }
            let subscribe = self.subscribe_many.lock().unwrap().clone();
            let unsubscribe = self.unsubscribe_many.lock().unwrap().clone();
            if !subscribe.is_empty() {
                panic!("Not all subscribe calls asserted: {subscribe:?}")
            }
            if !unsubscribe.is_empty() {
                panic!("Not all unsubscribe calls asserted: {unsubscribe:?}")
            }
        }
    }
}
