use bytes::BytesMut;
use futures::stream::StreamExt;
use futures::SinkExt;
use mqttbytes::v4::ConnAck;
use mqttbytes::v4::ConnectReturnCode;
use mqttbytes::v4::Disconnect;
use mqttbytes::v4::Packet;
use mqttbytes::v4::PingReq;
use mqttbytes::v4::PingResp;
use mqttbytes::v4::PubAck;
use mqttbytes::v4::PubComp;
use mqttbytes::v4::PubRec;
use mqttbytes::v4::Publish;
use mqttbytes::v4::SubAck;
use mqttbytes::v4::SubscribeReasonCode;
use mqttbytes::v4::UnsubAck;
use mqttbytes::QoS;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::atomic::AtomicU16;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::select;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tokio_util::codec::Decoder;
use tokio_util::codec::Encoder;
use tokio_util::codec::Framed;
use tracing::error;
use tracing::info;
use tracing::warn;

/// Custom MQTT Codec for use with `tokio_util::codec::Framed`.
/// This codec handles the encoding and decoding of MQTT `Packet`s.
struct MqttCodec;

impl Decoder for MqttCodec {
    type Item = Packet;
    type Error = std::io::Error;

    /// Decodes bytes from the source buffer into an MQTT Packet.
    /// It relies on `mqttbytes::v4::read` which handles the variable length header.
    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // Attempt to decode a packet from the source buffer.
        match mqttbytes::v4::read(src, 1024 * 1024) {
            Ok(packet) => {
                // Packet successfully decoded. Return it.
                Ok(Some(packet))
            }
            Err(mqttbytes::Error::InsufficientBytes(_)) => {
                // Not enough bytes in the buffer to form a complete packet.
                // Return `None` to indicate more data is needed.
                Ok(None)
            }
            Err(e) => {
                // Other decoding errors (e.g., malformed packet).
                // Convert to `std::io::Error` and propagate.
                Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    e.to_string(),
                ))
            }
        }
    }
}

fn write(packet: Packet, dst: &mut BytesMut) -> Result<usize, mqttbytes::Error> {
    match packet {
        Packet::ConnAck(p) => p.write(dst),
        Packet::Connect(p) => p.write(dst),
        Packet::Publish(p) => p.write(dst),
        Packet::PubAck(p) => p.write(dst),
        Packet::PubRec(p) => p.write(dst),
        Packet::PubRel(p) => p.write(dst),
        Packet::PubComp(p) => p.write(dst),
        Packet::Subscribe(p) => p.write(dst),
        Packet::SubAck(p) => p.write(dst),
        Packet::Unsubscribe(p) => p.write(dst),
        Packet::UnsubAck(p) => p.write(dst),
        Packet::PingReq => PingReq.write(dst),
        Packet::PingResp => PingResp.write(dst),
        Packet::Disconnect => Disconnect.write(dst),
    }
}

impl Encoder<Packet> for MqttCodec {
    type Error = std::io::Error;

    /// Encodes an MQTT Packet into the destination buffer.
    fn encode(&mut self, item: Packet, dst: &mut BytesMut) -> Result<(), Self::Error> {
        write(item, dst)
            .map(|_| ())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))
    }
}

struct TriggerDisconnect;

/// A client ID - channel pair
type ClientSender = (String, mpsc::Sender<Packet>);

/// Represents a simple MQTT broker for testing purposes.
pub struct TestMqttBroker {
    /// The port the broker is listening on
    port: u16,
    /// A shared, mutable list of all `Publish` packets received by the broker.
    received_publishes: Arc<Mutex<VecDeque<Publish>>>,
    /// A shared, mutable list of all `Publish` packets sent by the broker.
    sent_publishes: Arc<Mutex<VecDeque<Publish>>>,
    /// A shared, mutable list of all `PubAck` packets received by the broker.
    received_acks: Arc<Mutex<VecDeque<PubAck>>>,
    /// A flag to determine if the broker should acknowledge incoming publishes.
    should_acknowledge_publishes: Arc<Mutex<bool>>,
    /// Stores active subscriptions: String -> List of client Senders (for sending packets).
    subscriptions: Arc<Mutex<HashMap<String, Vec<ClientSender>>>>,
    /// Stores a map of client ID to their packet sender, for direct messaging or cleanup.
    client_senders: Arc<Mutex<HashMap<String, mpsc::Sender<Packet>>>>,
    /// The TCP listener that accepts incoming client connections.
    listener: TcpListener,
    /// The next packet id to be assigned to a published message
    pkid: AtomicU16,
    /// A list of channels via which we can close all connections to the broker
    disconnect_handles: Arc<Mutex<Vec<mpsc::Sender<TriggerDisconnect>>>>,
    /// A map to keep track of which messages are acknowledged, indexed by client id and pkid
    inflight: Arc<Mutex<HashMap<(String, u16), bool>>>,
}

impl TestMqttBroker {
    /// Creates a new `MqttBroker` instance, binding to a randomly allocated port on `127.0.0.1`.
    pub async fn new() -> Result<Self, std::io::Error> {
        // Bind to port 0 to let the OS assign a random available port.
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();

        info!("Broker listening on 127.0.0.1:{port}");

        Ok(TestMqttBroker {
            port,
            received_publishes: Arc::new(Mutex::new(VecDeque::new())),
            sent_publishes: Arc::new(Mutex::new(VecDeque::new())),
            received_acks: Arc::new(Mutex::new(VecDeque::new())),
            should_acknowledge_publishes: Arc::new(Mutex::new(true)), // Default to acknowledging
            subscriptions: Arc::new(Mutex::new(HashMap::new())),
            client_senders: Arc::new(Mutex::new(HashMap::new())),
            listener,
            pkid: AtomicU16::new(1),
            disconnect_handles: <_>::default(),
            inflight: <_>::default(),
        })
    }

    pub async fn disconnect_clients_abruptly(&self) {
        let mut handles = self.disconnect_handles.lock().await;
        for handle in &mut *handles {
            // If the client has already stopped, we don't care, so ignore the error
            let _ = handle.send(TriggerDisconnect).await;
        }

        // Wait for all the clients to be disconnected to avoid races where the
        // client is still connected
        for handle in &mut *handles {
            while !handle.is_closed() {
                tokio::task::yield_now().await;
            }
        }
    }

    /// Returns the port on which the broker is listening.
    pub fn port(&self) -> u16 {
        self.port
    }

    pub async fn next_message_matching(&self, filter: &str) -> Publish {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                {
                    let mut pubs = self.received_publishes.lock().await;
                    if let Some(msg) = pubs.pop_front() {
                        if mqttbytes::matches(&msg.topic, filter) {
                            return msg.clone();
                        } else {
                            tracing::warn!("Ignoring message: {msg:?}");
                        }
                    }
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("timed out waiting for message")
    }

    pub async fn wait_until_all_messages_acked(&self) {
        let mut last_acks = None;
        let res = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                {
                    let acks = self.inflight.lock().await;
                    if acks.values().all(|ack| *ack) {
                        break;
                    } else {
                        last_acks = Some(acks.clone())
                    }
                }
                tokio::task::yield_now().await;
            }
        })
        .await;

        if res.is_err() {
            let ids = last_acks
                .unwrap()
                .into_iter()
                .filter(|&(_, acknowledged)| !acknowledged)
                .map(|(key, _)| key)
                .collect::<Vec<_>>();
            panic!("Timed out waiting for all messages to be acknowledged. The missing acknowledgements (in the format `(client_id, pkid)` are: {ids:?}");
        }
    }

    /// Disables acknowledgements for incoming `Publish` messages.
    pub async fn disable_acknowledgements(&self) {
        let mut should_ack = self.should_acknowledge_publishes.lock().await;
        assert!(
            *should_ack,
            "Cannot disable acknowledgements as they are already disabled"
        );
        *should_ack = false;
    }

    /// (Re-)enables acknowledgements for incoming `Publish` messages.
    pub async fn enable_acknowledgements(&self) {
        let mut should_ack = self.should_acknowledge_publishes.lock().await;
        assert!(
            !*should_ack,
            "Cannot enable acknowledgements as they are already enabled"
        );
        *should_ack = true;
    }

    /// Returns a clone of the list of `Publish` packets sent by the broker.
    pub async fn sent_publishes(&self) -> VecDeque<Publish> {
        self.sent_publishes.lock().await.clone()
    }

    /// Returns a clone of the list of `Publish` packets received by the broker.
    pub async fn received_acks(&self) -> VecDeque<PubAck> {
        self.received_acks.lock().await.clone()
    }

    /// Publishes a message from the broker to all subscribed clients.
    /// This method can be called externally to simulate broker-initiated publishes.
    pub async fn publish_to_clients(
        &self,
        topic: &str,
        payload: &[u8],
        qos: QoS,
    ) -> Result<(), std::io::Error> {
        let mut publish_packet = Publish::new(topic, qos, payload);
        publish_packet.pkid = self.pkid.fetch_add(1, Ordering::SeqCst);
        info!("Broker initiating publish: {publish_packet:?}");
        self.sent_publishes
            .lock()
            .await
            .push_back(publish_packet.clone());

        let subscriptions_guard = self.subscriptions.lock().await;

        // Iterate through all subscriptions to find matching ones.
        for (sub_filter, client_senders) in subscriptions_guard.iter() {
            // Check if the topic matches the subscription filter.
            if mqttbytes::matches(sub_filter, topic) {
                // Iterate through all clients subscribed to this filter.
                for (client_id, sender) in client_senders.iter() {
                    // Attempt to send the publish packet to the client.
                    // If sending fails, it means the client's receiver is dropped,
                    // indicating a disconnected client. Mark the sender for removal.
                    if sender
                        .send(Packet::Publish(publish_packet.clone()))
                        .await
                        .is_err()
                    {
                        // Note: We can't remove from the map while iterating it.
                        // We'll need to handle cleanup outside this loop or with a more complex structure.
                        // For this simple broker, we'll just log the error.
                        error!("Failed to send publish to a client, likely disconnected.");
                        // In a real broker, you'd remove this sender from the subscription list.
                        // For simplicity here, we rely on the client handler to clean up its own sender.
                    }
                    self.inflight
                        .lock()
                        .await
                        .insert((client_id.clone(), publish_packet.pkid), false);
                }
            }
        }
        Ok(())
    }

    /// Starts the MQTT broker, listening for and handling incoming client connections.
    /// This method will loop indefinitely, accepting new connections.
    pub async fn start(&self) -> Result<(), std::io::Error> {
        loop {
            // Accept a new incoming TCP connection.
            let (stream, peer_addr) = self.listener.accept().await?;
            info!("Accepted connection from: {peer_addr}");

            // Clone the shared state for the new client handler task.
            let received_publishes_clone = Arc::clone(&self.received_publishes);
            let received_acks_clone = Arc::clone(&self.received_acks);
            let should_acknowledge_publishes_clone = Arc::clone(&self.should_acknowledge_publishes);
            let subscriptions_clone = Arc::clone(&self.subscriptions);
            let client_senders_clone = Arc::clone(&self.client_senders);
            let (disconnect_tx, disconnect_rx) = mpsc::channel(10);
            self.disconnect_handles.lock().await.push(disconnect_tx);
            let inflight_clone = Arc::clone(&self.inflight);

            // Spawn a new asynchronous task to handle this client connection independently.
            tokio::spawn(async move {
                if let Err(e) = Self::handle_client(
                    stream,
                    peer_addr,
                    received_publishes_clone,
                    received_acks_clone,
                    should_acknowledge_publishes_clone,
                    subscriptions_clone,
                    client_senders_clone,
                    disconnect_rx,
                    inflight_clone,
                )
                .await
                {
                    error!("Error handling client {}: {}", peer_addr, e);
                }
            });
        }
    }

    /// Handles a single client connection.
    /// This function reads MQTT packets from the client, processes them, and sends responses.
    #[allow(clippy::too_many_arguments)]
    async fn handle_client(
        stream: TcpStream,
        peer_addr: SocketAddr,
        received_publishes: Arc<Mutex<VecDeque<Publish>>>,
        received_acks: Arc<Mutex<VecDeque<PubAck>>>,
        should_acknowledge_publishes: Arc<Mutex<bool>>,
        subscriptions: Arc<Mutex<HashMap<String, Vec<ClientSender>>>>,
        client_senders: Arc<Mutex<HashMap<String, mpsc::Sender<Packet>>>>,
        mut disconnect: mpsc::Receiver<TriggerDisconnect>,
        inflight: Arc<Mutex<HashMap<(String, u16), bool>>>,
    ) -> Result<(), std::io::Error> {
        // Create a `Framed` instance to handle MQTT packet framing over the TCP stream.
        let mut framed = Framed::new(stream, MqttCodec);

        // Create an MPSC channel for this client to receive packets from the broker (e.g., published messages).
        let (tx, mut rx) = mpsc::channel::<Packet>(100);

        // The first packet from a client must be a CONNECT packet.
        let connect_packet = match framed.next().await {
            Some(Ok(Packet::Connect(c))) => c,
            Some(Ok(p)) => {
                error!("Client {peer_addr}: Received unexpected packet before CONNECT: {p:?}");
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Expected CONNECT packet",
                ));
            }
            Some(Err(e)) => return Err(e), // Propagate I/O errors from the stream
            None => {
                info!("Client {peer_addr} disconnected before sending CONNECT.");
                return Ok(()); // Client disconnected gracefully
            }
        };

        info!("Client {peer_addr}: Received CONNECT packet: {connect_packet:?}",);

        // Store client ID and its sender for future use.
        let client_id = connect_packet.client_id.clone();
        client_senders
            .lock()
            .await
            .insert(client_id.clone(), tx.clone());

        // Determine `session_present` flag for CONNACK based on `clean_session` from CONNECT.
        // If `clean_session` is true, the session is not present. Otherwise, it is.
        let session_present = !connect_packet.clean_session;
        // Create a CONNACK packet with success return code (0).
        let connack = ConnAck::new(ConnectReturnCode::Success, session_present);
        // Send the CONNACK packet back to the client.
        framed.send(Packet::ConnAck(connack.clone())).await?;
        info!("Client {client_id}: Sent CONNACK packet: {connack:?}");

        // Main loop for handling subsequent packets from the client and sending packets to the client.
        loop {
            select! {
                _ = disconnect.recv() => {
                    break;
                }
                // Branch 1: Read incoming packets from the client.
                client_msg = framed.next() => {
                    match client_msg {
                        Some(Ok(packet)) => {
                            info!("Client {client_id}: Received packet: {packet:?}");
                            match packet {
                                Packet::Publish(publish) => {
                                    // Store the received Publish packet in the shared list.
                                    received_publishes.lock().await.push_back(publish.clone());

                                    // If acknowledging is enabled, send appropriate ACK based on QoS.
                                    if *should_acknowledge_publishes.lock().await {
                                        let response_packet = match publish.qos {
                                            QoS::AtMostOnce => None, // No ACK for QoS 0
                                            QoS::AtLeastOnce => Some(Packet::PubAck(PubAck::new(publish.pkid))),
                                            QoS::ExactlyOnce => Some(Packet::PubRec(PubRec::new(publish.pkid))),
                                        };
                                        if let Some(resp) = response_packet {
                                            info!("Client {client_id}: Acknowledging: {resp:?}");
                                            framed.send(resp.clone()).await?;
                                        }
                                    }
                                }
                                Packet::PubAck(ack) => {
                                    received_acks.lock().await.push_back(ack.clone());
                                    let pkid = ack.pkid;
                                    if let Some(value) = inflight.lock().await.get_mut(&(client_id.clone(), ack.pkid)) {
                                        if *value {
                                            panic!("Broker received duplicate acknowledgment for {pkid} from {client_id}")
                                        } else {
                                            *value = true;
                                        }
                                    } else                                    {
                                        panic!("Broker received PubAck for unknown message {pkid} from {client_id}")
                                    }
                                }
                                Packet::Subscribe(subscribe) => {
                                    let mut return_codes = Vec::new();
                                    let mut subs_guard = subscriptions.lock().await;
                                    for topic_filter in subscribe.filters {
                                        info!("Client {client_id}: Subscribing to: {topic_filter:?}");
                                        return_codes.push(SubscribeReasonCode::Success(topic_filter.qos)); // Success return code
                                        // Add this client's sender to the list for this topic filter.
                                        subs_guard.entry(topic_filter.path)
                                            .or_insert_with(Vec::new)
                                            .push((client_id.clone(), tx.clone()));
                                    }
                                    let suback = SubAck::new(subscribe.pkid, return_codes);
                                    framed.send(Packet::SubAck(suback)).await?;
                                }
                                Packet::Unsubscribe(unsubscribe) => {
                                    let mut subs_guard = subscriptions.lock().await;
                                    for topic_filter in unsubscribe.topics {
                                        info!("Client {client_id}: Unsubscribing from: {topic_filter:?}");
                                        if let Some(senders) = subs_guard.get_mut(&topic_filter) {
                                            // Remove this client's sender from the list.
                                            senders.retain(|(_, s)| !s.same_channel(&tx));
                                            if senders.is_empty() {
                                                subs_guard.remove(&topic_filter);
                                            }
                                        }
                                    }
                                    let unsuback = UnsubAck::new(unsubscribe.pkid);
                                    framed.send(Packet::UnsubAck(unsuback)).await?;
                                }
                                Packet::PubRel(pubrel) => {
                                    // For QoS 2 flow, respond with PubComp after receiving PubRel.
                                    framed.send(Packet::PubComp(PubComp::new(pubrel.pkid))).await?;
                                }
                                Packet::PingReq => {
                                    framed.send(Packet::PingResp).await?;
                                }
                                Packet::Disconnect => {
                                    info!("Client {client_id}: Sent DISCONNECT. Closing connection.");
                                    break;
                                }
                                packet => {
                                    warn!("Client {client_id}: Ignoring unsupported packet type: {packet:?}");
                                }
                            }
                        }
                        Some(Err(e)) => {
                            error!("Client {client_id}: Error reading from stream: {e}");
                            break;
                        }
                        None => {
                            info!("Client {client_id}: Disconnected gracefully.");
                            break;
                        }
                    }
                }
                // Branch 2: Receive packets to send to the client (from broker's publish_to_clients).
                internal_msg = rx.recv() => {
                    match internal_msg {
                        Some(packet_to_send) => {
                            info!("Client {client_id}: Sending internal packet: {packet_to_send:?}");
                            if let Err(e) = framed.send(packet_to_send).await {
                                error!("Client {client_id}: Failed to send internal packet: {e}");
                                // If sending fails, the client might have disconnected.
                                break;
                            }
                        }
                        None => {
                            // The sender half of the channel was dropped, meaning the broker is shutting down
                            // or the client's sender was removed from all subscription lists.
                            info!("Client {client_id}: Internal message channel closed.");
                            break;
                        }
                    }
                }
            }
        }

        // Cleanup: Remove client's sender from shared maps when connection closes.
        client_senders.lock().await.remove(&client_id);
        let mut subs_guard = subscriptions.lock().await;
        for (_filter, senders) in subs_guard.iter_mut() {
            senders.retain(|(_, s)| !s.same_channel(&tx));
        }
        // Remove empty topic filter entries
        subs_guard.retain(|_filter, senders| !senders.is_empty());

        Ok(())
    }
}

mod tests {
    use super::*;
    use mqttbytes::v4::Connect;
    use mqttbytes::v4::PubRel;
    use mqttbytes::v4::Subscribe;
    use mqttbytes::v4::SubscribeFilter;
    use mqttbytes::v4::Unsubscribe;
    use tokio::net::TcpStream;
    use tokio::time::timeout;
    use tokio::time::Duration;

    /// Helper function to establish a framed MQTT client connection to the broker.
    async fn connect_to_broker(
        port: u16,
    ) -> Result<Framed<TcpStream, MqttCodec>, Box<dyn std::error::Error>> {
        let stream = TcpStream::connect(format!("127.0.0.1:{}", port)).await?;
        Ok(Framed::new(stream, MqttCodec))
    }

    /// Helper to connect and consume the CONNACK
    async fn connect_and_ack(
        client: &mut Framed<TcpStream, MqttCodec>,
        client_id: &str,
        clean_session: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut connect = Connect::new(client_id);
        connect.clean_session = clean_session;
        client.send(Packet::Connect(connect)).await?;
        let _ = timeout(Duration::from_secs(1), client.next())
            .await?
            .ok_or("Client disconnected unexpectedly during CONNACK")??;
        Ok(())
    }

    #[tokio::test]
    async fn test_broker_binds_random_port() -> Result<(), Box<dyn std::error::Error>> {
        let broker = TestMqttBroker::new().await?;
        let port = broker.port();
        assert!(port > 0);
        Ok(())
    }

    #[tokio::test]
    async fn test_broker_connect_connack_clean_session_true(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let broker = TestMqttBroker::new().await?;
        let port = broker.port();
        tokio::spawn(async move {
            broker.start().await.unwrap();
        });

        let mut client = connect_to_broker(port).await?;
        let mut connect = Connect::new("test_client");
        connect.clean_session = true;
        client.send(Packet::Connect(connect)).await?;
        let connack_packet = timeout(Duration::from_secs(1), client.next())
            .await?
            .ok_or("Client disconnected")??;

        if let Packet::ConnAck(connack) = connack_packet {
            assert_eq!(connack.code, ConnectReturnCode::Success);
            assert!(!connack.session_present);
        } else {
            panic!("Expected CONNACK, got {:?}", connack_packet);
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_broker_connect_connack_clean_session_false(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let broker = TestMqttBroker::new().await?;
        let port = broker.port();
        tokio::spawn(async move {
            broker.start().await.unwrap();
        });

        let mut client = connect_to_broker(port).await?;
        let mut connect = Connect::new("test_client");
        connect.clean_session = false;
        client.send(Packet::Connect(connect)).await?;
        let connack_packet = timeout(Duration::from_secs(1), client.next())
            .await?
            .ok_or("Client disconnected")??;

        if let Packet::ConnAck(connack) = connack_packet {
            assert_eq!(connack.code, ConnectReturnCode::Success);
            assert!(connack.session_present);
        } else {
            panic!("Expected CONNACK, got {:?}", connack_packet);
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_broker_receives_publish_qos0() -> Result<(), Box<dyn std::error::Error>> {
        let broker = TestMqttBroker::new().await?;
        let port = broker.port();
        let broker_publishes = broker.received_publishes.clone();
        tokio::spawn(async move {
            broker.start().await.unwrap();
        });

        let mut client = connect_to_broker(port).await?;
        connect_and_ack(&mut client, "test_client_qos0", true).await?;

        let publish = Publish::new("test/topic", QoS::AtMostOnce, "hello");
        client.send(Packet::Publish(publish.clone())).await?;
        tokio::time::sleep(Duration::from_millis(100)).await;

        let received = broker_publishes.lock().await;
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].topic, "test/topic");
        assert_eq!(&*received[0].payload, b"hello");
        assert_eq!(received[0].qos, QoS::AtMostOnce);

        let next_packet = timeout(Duration::from_millis(100), client.next()).await;
        assert!(next_packet.is_err() || next_packet.unwrap().is_none());
        Ok(())
    }

    #[tokio::test]
    async fn test_broker_receives_publish_qos1() -> Result<(), Box<dyn std::error::Error>> {
        let broker = TestMqttBroker::new().await?;
        let port = broker.port();
        let broker_publishes = broker.received_publishes.clone();
        tokio::spawn(async move {
            broker.start().await.unwrap();
        });

        let mut client = connect_to_broker(port).await?;
        connect_and_ack(&mut client, "test_client_qos1", true).await?;

        let mut publish = Publish::new("test/topic", QoS::AtLeastOnce, "world");
        publish.pkid = 1;
        client.send(Packet::Publish(publish.clone())).await?;

        let puback_packet = timeout(Duration::from_secs(1), client.next())
            .await?
            .ok_or("Client disconnected")??;
        if let Packet::PubAck(puback) = puback_packet {
            assert_eq!(puback.pkid, 1);
        } else {
            panic!("Expected PUBACK, got {:?}", puback_packet);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;

        let received = broker_publishes.lock().await;
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].topic, "test/topic");
        assert_eq!(&*received[0].payload, b"world");
        assert_eq!(received[0].qos, QoS::AtLeastOnce);
        Ok(())
    }

    #[tokio::test]
    async fn test_broker_receives_publish_qos2() -> Result<(), Box<dyn std::error::Error>> {
        let broker = TestMqttBroker::new().await?;
        let port = broker.port();
        let broker_publishes = Arc::clone(&broker.received_publishes);
        tokio::spawn(async move {
            broker.start().await.unwrap();
        });

        let mut client = connect_to_broker(port).await?;
        connect_and_ack(&mut client, "test_client_qos2", true).await?;

        let mut publish = Publish::new("test/topic", QoS::ExactlyOnce, "qos2_message");
        publish.pkid = 2;
        client.send(Packet::Publish(publish.clone())).await?;

        let pubrec_packet = timeout(Duration::from_secs(1), client.next())
            .await?
            .ok_or("Client disconnected")??;
        if let Packet::PubRec(pubrec) = pubrec_packet {
            assert_eq!(pubrec.pkid, 2);
        } else {
            panic!("Expected PUBREC, got {:?}", pubrec_packet);
        }

        let pubrel = PubRel::new(2);
        client.send(Packet::PubRel(pubrel)).await?;

        let pubcomp_packet = timeout(Duration::from_secs(1), client.next())
            .await?
            .ok_or("Client disconnected")??;
        if let Packet::PubComp(pubcomp) = pubcomp_packet {
            assert_eq!(pubcomp.pkid, 2);
        } else {
            panic!("Expected PUBCOMP, got {:?}", pubcomp_packet);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;

        let received = broker_publishes.lock().await;
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].topic, "test/topic");
        assert_eq!(&*received[0].payload, b"qos2_message");
        assert_eq!(received[0].qos, QoS::ExactlyOnce);
        Ok(())
    }

    #[tokio::test]
    async fn test_broker_acknowledges_publishes_flag() -> Result<(), Box<dyn std::error::Error>> {
        let broker = TestMqttBroker::new().await?;
        let port = broker.port();
        broker.disable_acknowledgements().await;

        tokio::spawn(async move {
            broker.start().await.unwrap();
        });

        let mut client = connect_to_broker(port).await?;
        connect_and_ack(&mut client, "test_client_no_ack", true).await?;

        let mut publish = Publish::new("test/topic", QoS::AtLeastOnce, "no_ack_message");
        publish.pkid = 3;
        client.send(Packet::Publish(publish.clone())).await?;

        let next_packet = timeout(Duration::from_millis(100), client.next()).await;
        assert!(next_packet.is_err() || next_packet.unwrap().is_none()); // Expect timeout or None
        Ok(())
    }

    #[tokio::test]
    async fn test_broker_acknowledges_publishes_flag_after_chn(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let broker = Arc::new(TestMqttBroker::new().await?);
        let port = broker.port();
        broker.disable_acknowledgements().await;

        {
            let broker = broker.clone();
            tokio::spawn(async move {
                broker.start().await.unwrap();
            });
        }

        let mut client = connect_to_broker(port).await?;
        connect_and_ack(&mut client, "test_client_no_ack", true).await?;

        let mut publish = Publish::new("test/topic", QoS::AtLeastOnce, "no_ack_message");
        publish.pkid = 3;
        client.send(Packet::Publish(publish.clone())).await?;

        let next_packet = timeout(Duration::from_millis(100), client.next()).await;
        assert!(next_packet.is_err() || next_packet.unwrap().is_none()); // Expect timeout or None

        broker.enable_acknowledgements().await;
        publish.pkid = 4;
        client.send(Packet::Publish(publish.clone())).await?;
        let puback_packet = timeout(Duration::from_millis(100), client.next())
            .await?
            .unwrap()?;
        if let Packet::PubAck(puback) = puback_packet {
            assert_eq!(puback.pkid, 4);
        } else {
            panic!("Expected PUBACK, got {:?}", puback_packet);
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_broker_ping_pong() -> Result<(), Box<dyn std::error::Error>> {
        let broker = TestMqttBroker::new().await?;
        let port = broker.port();
        tokio::spawn(async move {
            broker.start().await.unwrap();
        });

        let mut client = connect_to_broker(port).await?;
        connect_and_ack(&mut client, "test_client_ping", true).await?;

        client.send(Packet::PingReq).await?;
        let pingresp_packet = timeout(Duration::from_secs(1), client.next())
            .await?
            .ok_or("Client disconnected")??;
        if let Packet::PingResp = pingresp_packet {
            // Success
        } else {
            panic!("Expected PINGRESP, got {:?}", pingresp_packet);
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_broker_disconnect() -> Result<(), Box<dyn std::error::Error>> {
        let broker = TestMqttBroker::new().await?;
        let port = broker.port();
        tokio::spawn(async move {
            broker.start().await.unwrap();
        });

        let mut client = connect_to_broker(port).await?;
        connect_and_ack(&mut client, "test_client_disconnect", true).await?;

        client.send(Packet::Disconnect).await?;
        let result = timeout(Duration::from_secs(1), client.next()).await;
        assert!(result.is_err() || result.unwrap().is_none());
        Ok(())
    }

    #[tokio::test]
    async fn test_broker_forwards_publish_to_subscribed_client(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let broker = Arc::new(TestMqttBroker::new().await?); // Wrap in Arc for shared ownership
        let port = broker.port();
        {
            let broker = broker.clone();
            tokio::spawn(async move { broker.start().await }); // Spawn the broker
        }

        let mut client1 = connect_to_broker(port).await?;
        connect_and_ack(&mut client1, "client1", true).await?;

        // Client 1 subscribes to "test/topic"
        let mut subscribe_packet = Subscribe::new("test/topic", QoS::AtLeastOnce);
        subscribe_packet.pkid = 1;
        client1.send(Packet::Subscribe(subscribe_packet)).await?;
        let suback_packet = timeout(Duration::from_secs(1), client1.next())
            .await?
            .ok_or("Client 1 disconnected")??;
        assert!(matches!(suback_packet, Packet::SubAck(_)));

        // Broker publishes a message to "test/topic"
        let test_payload = b"message_from_broker";
        broker
            .publish_to_clients("test/topic", test_payload, QoS::AtLeastOnce)
            .await?;

        // Client 1 should receive the message
        let received_packet = timeout(Duration::from_secs(1), client1.next())
            .await?
            .ok_or("Client 1 disconnected")??;
        if let Packet::Publish(publish) = received_packet {
            assert_eq!(publish.topic, "test/topic");
            assert_eq!(&*publish.payload, test_payload);
            assert_eq!(publish.qos, QoS::AtLeastOnce);
        } else {
            panic!("Client 1 expected PUBLISH, got {:?}", received_packet);
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_broker_does_not_forward_to_unsubscribed_client(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let broker = Arc::new(TestMqttBroker::new().await?);
        let port = broker.port();
        {
            let broker = broker.clone();
            tokio::spawn(async move { broker.start().await }); // Spawn the broker
        }

        let mut client1 = connect_to_broker(port).await?;
        connect_and_ack(&mut client1, "client1", true).await?;

        // Client 1 subscribes to "another/topic", not "test/topic"
        let subscribe_packet = Subscribe::new("another/topic", QoS::AtLeastOnce);
        client1.send(Packet::Subscribe(subscribe_packet)).await?;
        let suback_packet = timeout(Duration::from_secs(1), client1.next())
            .await?
            .ok_or("Client 1 disconnected")??;
        assert!(matches!(suback_packet, Packet::SubAck(_)));

        // Broker publishes a message to "test/topic"
        let test_payload = b"message_from_broker";
        broker
            .publish_to_clients("test/topic", test_payload, QoS::AtLeastOnce)
            .await?;

        // Client 1 should NOT receive the message (expect timeout)
        let received_packet = timeout(Duration::from_millis(200), client1.next()).await;
        assert!(received_packet.is_err() || received_packet.unwrap().is_none());
        Ok(())
    }

    #[tokio::test]
    async fn test_broker_handles_multiple_subscribers() -> Result<(), Box<dyn std::error::Error>> {
        let broker = Arc::new(TestMqttBroker::new().await?);
        let port = broker.port();
        {
            let broker = broker.clone();
            tokio::spawn(async move { broker.start().await }); // Spawn the broker
        }

        let mut client1 = connect_to_broker(port).await?;
        connect_and_ack(&mut client1, "client1", true).await?;
        let mut subscribe_packet1 = Subscribe::new("shared/topic", QoS::AtLeastOnce);
        subscribe_packet1.pkid = 1;
        client1.send(Packet::Subscribe(subscribe_packet1)).await?;
        let _ = timeout(Duration::from_secs(1), client1.next())
            .await?
            .ok_or("Client 1 disconnected")??;

        let mut client2 = connect_to_broker(port).await?;
        connect_and_ack(&mut client2, "client2", true).await?;
        let mut subscribe_packet2 = Subscribe::new("shared/topic", QoS::AtLeastOnce);
        subscribe_packet2.pkid = 2;
        client2.send(Packet::Subscribe(subscribe_packet2)).await?;
        let _ = timeout(Duration::from_secs(1), client2.next())
            .await?
            .ok_or("Client 2 disconnected")??;

        // Broker publishes a message to "shared/topic"
        let test_payload = b"shared_message";
        broker
            .publish_to_clients("shared/topic", test_payload, QoS::AtLeastOnce)
            .await?;

        // Both clients should receive the message
        let received_packet1 = timeout(Duration::from_secs(1), client1.next())
            .await?
            .ok_or("Client 1 disconnected")??;
        if let Packet::Publish(publish) = received_packet1 {
            assert_eq!(publish.topic, "shared/topic");
            assert_eq!(&*publish.payload, test_payload);
        } else {
            panic!("Client 1 expected PUBLISH, got {:?}", received_packet1);
        }

        let received_packet2 = timeout(Duration::from_secs(1), client2.next())
            .await?
            .ok_or("Client 2 disconnected")??;
        if let Packet::Publish(publish) = received_packet2 {
            assert_eq!(publish.topic, "shared/topic");
            assert_eq!(&*publish.payload, test_payload);
        } else {
            panic!("Client 2 expected PUBLISH, got {:?}", received_packet2);
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_broker_unsubscribes_client() -> Result<(), Box<dyn std::error::Error>> {
        let broker = Arc::new(TestMqttBroker::new().await?);
        let port = broker.port();
        {
            let broker = broker.clone();
            tokio::spawn(async move { broker.start().await }); // Spawn the broker
        }

        let mut client1 = connect_to_broker(port).await?;
        connect_and_ack(&mut client1, "client1", true).await?;

        let topic_filter = SubscribeFilter::new("test/topic".into(), QoS::AtLeastOnce);
        let subscribe_packet = Subscribe::new_many([topic_filter.clone()]);
        client1.send(Packet::Subscribe(subscribe_packet)).await?;
        let _ = timeout(Duration::from_secs(1), client1.next())
            .await?
            .ok_or("Client 1 disconnected")??;

        // Verify initial subscription
        {
            let subs_guard = broker.subscriptions.lock().await;
            assert!(subs_guard
                .get(&topic_filter.path)
                .is_some_and(|senders| !senders.is_empty()));
        }

        // Client 1 unsubscribes
        let unsubscribe_packet = Unsubscribe::new(topic_filter.path.clone());
        client1
            .send(Packet::Unsubscribe(unsubscribe_packet))
            .await?;
        let unsuback_packet = timeout(Duration::from_secs(1), client1.next())
            .await?
            .ok_or("Client 1 disconnected")??;
        assert!(matches!(unsuback_packet, Packet::UnsubAck(_)));

        // Give broker time to process
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Verify unsubscription
        {
            let subs_guard = broker.subscriptions.lock().await;
            assert!(subs_guard
                .get(&topic_filter.path)
                .map_or(true, |senders| senders.is_empty())); // Should be empty or removed
        }

        // Broker publishes a message to "test/topic"
        let test_payload = b"message_after_unsubscribe";
        broker
            .publish_to_clients("test/topic", test_payload, QoS::AtLeastOnce)
            .await?;

        // Client 1 should NOT receive the message (expect timeout)
        let received_packet = timeout(Duration::from_millis(200), client1.next()).await;
        assert!(received_packet.is_err() || received_packet.unwrap().is_none());
        Ok(())
    }
}
