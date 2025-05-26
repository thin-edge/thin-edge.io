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
use mqtt_channel::SubscriberOps;
pub use mqtt_channel::Topic;
pub use mqtt_channel::TopicFilter;
use std::convert::Infallible;
use std::sync::Arc;
use std::sync::Mutex;
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
pub use trie::SubscriptionDiff;

pub type MqttConfig = mqtt_channel::Config;

pub struct MqttActorBuilder {
    mqtt_config: mqtt_channel::Config,
    input_receiver: InputCombiner,
    pub_or_sub_sender: PubOrSubSender,
    publish_sender: mpsc::Sender<MqttMessage>,
    subscriber_addresses: Vec<DynSender<MqttMessage>>,
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

impl PublishOrSubscribe {
    pub fn subscribe(client_id: ClientId, diff: SubscriptionDiff) -> Self {
        PublishOrSubscribe::Subscribe(SubscriptionRequest { diff, client_id })
    }
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

impl MessageSource<MqttMessage, DynSubscriptions> for MqttActorBuilder {
    fn connect_sink(
        &mut self,
        subscriptions: DynSubscriptions,
        peer: &impl MessageSink<MqttMessage>,
    ) {
        let client_id = self.connect_id_sink(subscriptions.init_topics(), peer);
        subscriptions.set_client_id(client_id);
    }
}

#[derive(Clone)]
pub struct DynSubscriptions {
    inner: Arc<Mutex<DynSubscriptionsInner>>,
}
pub struct DynSubscriptionsInner {
    init_topics: TopicFilter,
    client_id: Option<ClientId>,
}

impl DynSubscriptions {
    pub fn new(init_topics: TopicFilter) -> Self {
        let inner = DynSubscriptionsInner {
            init_topics,
            client_id: None,
        };
        DynSubscriptions {
            inner: Arc::new(Mutex::new(inner)),
        }
    }

    fn set_client_id(&self, client_id: ClientId) {
        let mut inner = self.inner.lock().unwrap();
        inner.client_id = Some(client_id);
    }

    fn init_topics(&self) -> TopicFilter {
        self.inner.lock().unwrap().init_topics.clone()
    }

    /// Return the client id
    ///
    /// Panic if not properly registered as a sink of the MqttActorBuilder
    pub fn client_id(&self) -> ClientId {
        self.inner.lock().unwrap().client_id.unwrap()
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
    peer_senders: Vec<DynSender<MqttMessage>>,
    subscriptions: ClientMessageBox<TrieRequest, TrieResponse>,
}

impl FromPeers {
    async fn relay_messages_to(
        &mut self,
        outgoing_mqtt: &mut mpsc::UnboundedSender<MqttMessage>,
        client: impl SubscriberOps + Clone + Send + 'static,
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
                        // We're running outside the main task, so we can't return an error
                        // In practice, this should never fail
                        if !diff.subscribe.is_empty() {
                            client.subscribe_many(diff.subscribe).await.unwrap();
                        }
                        if !diff.unsubscribe.is_empty() {
                            client.unsubscribe_many(diff.unsubscribe).await.unwrap();
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
            self.peer_senders[client.0].send(message.clone()).await?;
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
        peer_senders: Vec<DynSender<MqttMessage>>,
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
                .relay_messages_to(&mut mqtt_client.published, mqtt_client.subscriptions),
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
        )
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
    }

    struct MqttActorTest {
        subscribe_client: MockSubscriberOps,
        sub_tx: mpsc::Sender<SubscriptionRequest>,
        pub_tx: mpsc::Sender<MqttMessage>,
        sent_to_channel: mpsc::UnboundedReceiver<MqttMessage>,
        sent_to_clients: HashMap<usize, mpsc::Receiver<MqttMessage>>,
        inject_received_message: mpsc::UnboundedSender<MqttMessage>,
    }

    impl MqttActorTest {
        pub fn new(default_subscriptions: &[(&str, usize)]) -> Self {
            let (pub_tx, pub_rx) = mpsc::channel(10);
            let (sub_tx, sub_rx) = mpsc::channel(10);
            let (_sig_tx, sig_rx) = mpsc::channel(10);
            let (mut outgoing_mqtt, sent_messages) = mpsc::unbounded();
            let (inject_received_message, mut incoming_messages) = mpsc::unbounded();
            let input_combiner = InputCombiner {
                publish_receiver: pub_rx,
                subscription_request_receiver: sub_rx,
                signal_receiver: sig_rx,
            };

            let mut ts = TrieService::with_default_subscriptions(default_subscriptions);
            let mut fp = FromPeers {
                input_receiver: input_combiner,
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

            let subscribe_client = MockSubscriberOps::default();
            {
                let client = subscribe_client.clone();
                tokio::spawn(async move { fp.relay_messages_to(&mut outgoing_mqtt, client).await });
            }
            tokio::spawn(async move { tp.relay_messages_from(&mut incoming_messages).await });

            Self {
                subscribe_client,
                sub_tx,
                pub_tx,
                sent_to_clients,
                sent_to_channel: sent_messages,
                inject_received_message,
            }
        }

        /// Simulates a client sending a subscription request to the mqtt actor
        pub async fn send_sub(&mut self, req: SubscriptionRequest) {
            SinkExt::send(&mut self.sub_tx, req).await.unwrap();
        }

        pub async fn publish(&mut self, topic: &str, payload: &str) {
            SinkExt::send(
                &mut self.pub_tx,
                MqttMessage::new(&Topic::new(topic).unwrap(), payload),
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
