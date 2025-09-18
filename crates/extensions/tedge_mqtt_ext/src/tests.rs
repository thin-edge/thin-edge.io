use crate::*;
use mqtt_channel::Topic;
use std::future::Future;
use std::time::Duration;
use tedge_actors::Builder;
use tedge_actors::NoConfig;
use tedge_actors::SimpleMessageBox;
use tedge_actors::SimpleMessageBoxBuilder;

type MqttClient = SimpleMessageBox<MqttMessage, MqttMessage>;

struct MqttClientBuilder {
    subscriptions: TopicFilter,
    box_builder: SimpleMessageBoxBuilder<MqttMessage, MqttMessage>,
}

impl MqttClientBuilder {
    fn new(name: &str, subscriptions: &(impl Clone + Into<TopicFilter>)) -> Self {
        MqttClientBuilder {
            subscriptions: subscriptions.clone().into(),
            box_builder: SimpleMessageBoxBuilder::new(name, 16),
        }
    }

    fn new_with_capacity(
        name: &str,
        subscriptions: &(impl Clone + Into<TopicFilter>),
        capacity: usize,
    ) -> Self {
        MqttClientBuilder {
            subscriptions: subscriptions.clone().into(),
            box_builder: SimpleMessageBoxBuilder::new(name, capacity),
        }
    }

    fn with_connection(
        self,
        mqtt: &mut (impl MessageSink<MqttMessage> + MessageSource<MqttMessage, TopicFilter>),
    ) -> Self {
        let box_builder = self
            .box_builder
            .with_connection(self.subscriptions.clone(), mqtt);
        MqttClientBuilder {
            subscriptions: self.subscriptions,
            box_builder,
        }
    }
}

impl Builder<MqttClient> for MqttClientBuilder {
    type Error = Infallible;

    fn try_build(self) -> Result<MqttClient, Self::Error> {
        Ok(self.build())
    }

    fn build(self) -> MqttClient {
        self.box_builder.build()
    }
}

#[tokio::test]
async fn mqtt_actor_can_reliably_forward_messages_under_load() {
    let broker = mqtt_tests::test_mqtt_broker();
    let mqtt_config = MqttConfig::default().with_port(broker.port);
    let mut mqtt = MqttActorBuilder::new(mqtt_config);

    let mut child_device = MqttClientBuilder::new("Child device", &TopicFilter::empty())
        .with_connection(&mut mqtt)
        .build();

    let c8y_mapper_topic = Topic::new_unchecked("some/tedge/topic");

    // Simulate a mapper with no message buffer, which will make the problem
    // occur more quickly if it exists
    let mut c8y_mapper = MqttClientBuilder::new_with_capacity("C8y mapper", &c8y_mapper_topic, 0)
        .with_connection(&mut mqtt)
        .build();

    // Assume Cumulocity just accepts all messages, so don't bother attaching a client
    let c8y_topic = Topic::new_unchecked("c8y/s/us");

    // Simulate the c8y mapper, forwarding messages from the child device to Cumulocity
    tokio::spawn(async move {
        while let Some(mut msg) = c8y_mapper.recv().await {
            tokio::time::sleep(Duration::from_secs(5)).await;
            msg.topic = c8y_topic.clone();
            // If the actor doesn't process incoming/outgoing MQTT messages concurrently,
            // this will cause a deadlock
            c8y_mapper.send(msg).await.unwrap();
        }
    });

    tokio::spawn(mqtt_actor(mqtt));

    for _ in 1..100 {
        // This timeout should only be triggered if the actor isn't progressing
        tokio::time::timeout(
            Duration::from_millis(50),
            child_device.send(MqttMessage::new(&c8y_mapper_topic, "Hi Bob")),
        )
            .await
            .expect("messages should be forwarded (is the MQTT actor processing incoming/outgoing messages concurrently?)")
            .expect("send should succeed");
    }
}

#[tokio::test]
async fn communicate_over_mqtt() {
    let broker = mqtt_tests::test_mqtt_broker();
    let mqtt_config = MqttConfig::default().with_port(broker.port);
    let mut mqtt = MqttActorBuilder::new(mqtt_config);

    let alice_topic = Topic::new_unchecked("messages/for/alice");
    let mut alice: MqttClient = MqttClientBuilder::new("Alice", &alice_topic)
        .with_connection(&mut mqtt)
        .build();

    let bob_topic = Topic::new_unchecked("messages/for/bob");
    let mut bob: MqttClient = MqttClientBuilder::new("Bob", &bob_topic)
        .with_connection(&mut mqtt)
        .build();

    let mut all_topics = TopicFilter::empty();
    all_topics.add_all(alice_topic.clone().into());
    all_topics.add_all(bob_topic.clone().into());
    let mut spy: MqttClient = MqttClientBuilder::new("Spy", &all_topics)
        .with_connection(&mut mqtt)
        .build();

    tokio::spawn(mqtt_actor(mqtt));

    // Message pub on some topic is received by all subscribers
    assert!(alice
        .send(MqttMessage::new(&bob_topic, "Hi Bob"))
        .await
        .is_ok());
    assert_eq!(
        timeout(bob.recv()).await,
        Some(MqttMessage::new(&bob_topic, "Hi Bob"))
    );
    assert_eq!(
        timeout(spy.recv()).await,
        Some(MqttMessage::new(&bob_topic, "Hi Bob"))
    );

    // Messages from misc clients can be published without awaiting for a response
    assert!(bob.send(MqttMessage::new(&alice_topic, "1")).await.is_ok());
    assert!(bob.send(MqttMessage::new(&alice_topic, "2")).await.is_ok());
    assert!(alice.send(MqttMessage::new(&bob_topic, "A")).await.is_ok());
    assert!(bob.send(MqttMessage::new(&alice_topic, "3")).await.is_ok());
    assert!(alice.send(MqttMessage::new(&bob_topic, "B")).await.is_ok());
    assert!(alice.send(MqttMessage::new(&bob_topic, "C")).await.is_ok());

    // A subscriber receives only the messages for its subscriptions
    assert_eq!(
        timeout(bob.recv()).await,
        Some(MqttMessage::new(&bob_topic, "A"))
    );
    assert_eq!(
        timeout(bob.recv()).await,
        Some(MqttMessage::new(&bob_topic, "B"))
    );
    assert_eq!(
        timeout(bob.recv()).await,
        Some(MqttMessage::new(&bob_topic, "C"))
    );

    // When messages are pub/sub on a single topic; they are received in order
    assert_eq!(
        timeout(alice.recv()).await,
        Some(MqttMessage::new(&alice_topic, "1"))
    );
    assert_eq!(
        timeout(alice.recv()).await,
        Some(MqttMessage::new(&alice_topic, "2"))
    );
    assert_eq!(
        timeout(alice.recv()).await,
        Some(MqttMessage::new(&alice_topic, "3"))
    );

    // However when messages are sent by different actors or over several topics
    // Message order can be altered.
    let mut messages = vec![];
    for _i in 0..6 {
        let message = spy.recv().await.expect("some message");
        messages.push(message.payload_str().expect("utf8").to_string());
    }
    messages.sort();
    assert_eq!(messages, vec!["1", "2", "3", "A", "B", "C"])
}

#[tokio::test]
async fn dynamic_subscriptions() {
    let broker = mqtt_tests::test_mqtt_broker();
    let mqtt_config = MqttConfig::default().with_port(broker.port);
    let mut mqtt = MqttActorBuilder::new(mqtt_config);

    let mut client_0 = SimpleMessageBoxBuilder::<_, MqttRequest>::new("dyn-subscriber", 16);
    let mut client_1 = SimpleMessageBoxBuilder::<_, MqttRequest>::new("dyn-subscriber1", 16);
    let client_id_0 = mqtt.connect_id_sink(TopicFilter::new_unchecked("a/b"), &client_0);
    let _client_id_1 = mqtt.connect_id_sink(TopicFilter::new_unchecked("a/+"), &client_1);
    client_0.connect_sink(NoConfig, &mqtt);
    client_1.connect_sink(NoConfig, &mqtt);
    let mqtt = mqtt.build();
    tokio::spawn(async move { mqtt.run().await.unwrap() });
    let mut client_0 = client_0.build();
    let mut client_1 = client_1.build();

    let msg = MqttMessage::new(&Topic::new_unchecked("a/b"), "hello");
    client_0
        .send(MqttRequest::Publish(msg.clone()))
        .await
        .unwrap();
    assert_eq!(timeout(client_0.recv()).await.unwrap(), msg);
    assert_eq!(timeout(client_1.recv()).await.unwrap(), msg);

    // Send the messages as retain so we don't have a race for the subscription
    let msg = MqttMessage::new(&Topic::new_unchecked("b/c"), "hello").with_retain();
    client_0
        .send(MqttRequest::Publish(msg.clone()))
        .await
        .unwrap();

    client_0
        .send(MqttRequest::Subscribe(SubscriptionRequest {
            diff: SubscriptionDiff {
                subscribe: ["b/c".into()].into(),
                unsubscribe: [].into(),
            },
            client_id: client_id_0,
        }))
        .await
        .unwrap();

    assert_eq!(timeout(client_0.recv()).await.unwrap(), msg);

    // Verify that messages aren't sent to clients
    let msg = MqttMessage::new(&Topic::new_unchecked("a/c"), "hello");
    client_0
        .send(MqttRequest::Publish(msg.clone()))
        .await
        .unwrap();
    assert_eq!(timeout(client_1.recv()).await.unwrap(), msg);
    assert!(
        tokio::time::timeout(Duration::from_millis(10), client_0.recv())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn client_with_overlapping_subscriptions() {
    let broker = mqtt_tests::test_mqtt_broker();
    let mqtt_config = MqttConfig::default().with_port(broker.port);
    let mut mqtt = MqttActorBuilder::new(mqtt_config);
    let mut publisher = SimpleMessageBoxBuilder::<MqttRequest, _>::new("publisher", 16);
    publisher.connect_sink(NoConfig, &mqtt);

    // A client starts with a single subscription
    let mut client = SimpleMessageBoxBuilder::<_, MqttRequest>::new("dyn-subscriber", 16);
    let client_id = mqtt.connect_id_sink(TopicFilter::new_unchecked("a/+"), &client);
    client.connect_sink(NoConfig, &mqtt);

    let mqtt = mqtt.build();
    tokio::spawn(async move { mqtt.run().await.unwrap() });
    let mut publisher = publisher.build();
    let mut client = client.build();

    // The client then dynamically subscribes to an overlapping topic
    client
        .send(MqttRequest::Subscribe(SubscriptionRequest {
            diff: SubscriptionDiff {
                // a more convincing example would be to subscribe to "+/b"
                // but in that case rumqttd sends the message twice
                // something we can do little about
                subscribe: ["a/b".into()].into(),
                unsubscribe: [].into(), // still wants to receive messages from "a/+"
            },
            client_id,
        }))
        .await
        .unwrap();

    // A message published on the overlap ...
    let msg = MqttMessage::new(&Topic::new_unchecked("a/b"), "hello");
    publisher
        .send(MqttRequest::Publish(msg.clone()))
        .await
        .unwrap();

    // ... should then not be received twice
    assert_eq!(timeout(client.recv()).await.unwrap(), msg);
    if let Ok(Some(duplicate)) =
        tokio::time::timeout(Duration::from_millis(10), client.recv()).await
    {
        if duplicate == msg {
            panic!("A message has been delivered twice to the subscriber");
        }
    }
}

#[tokio::test]
async fn dynamic_subscribers_receive_retain_messages() {
    let broker = mqtt_tests::test_mqtt_broker();
    let mqtt_config = MqttConfig::default()
        .with_port(broker.port)
        .with_session_name("test");
    let mut mqtt = MqttActorBuilder::new(mqtt_config);

    broker
        .publish_with_opts("a/b", "retain", QoS::AtLeastOnce, true)
        .await
        .unwrap();
    broker
        .publish_with_opts("b/c", "retain", QoS::AtLeastOnce, true)
        .await
        .unwrap();

    let mut client_0 = SimpleMessageBoxBuilder::<_, MqttRequest>::new("dyn-subscriber", 16);
    let mut client_1 = SimpleMessageBoxBuilder::<_, MqttRequest>::new("dyn-subscriber1", 16);
    let _client_id_0 = mqtt.connect_id_sink(TopicFilter::new_unchecked("a/b"), &client_0);
    let client_id_1 = mqtt.connect_id_sink(TopicFilter::empty(), &client_1);
    client_0.connect_sink(NoConfig, &mqtt);
    client_1.connect_sink(NoConfig, &mqtt);
    let mqtt = mqtt.build();
    tokio::spawn(async move { mqtt.run().await.unwrap() });
    let mut client_0 = client_0.build();
    let mut client_1 = client_1.build();

    let msg = MqttMessage::new(&Topic::new_unchecked("a/b"), "retain").with_retain();
    let msg2 = MqttMessage::new(&Topic::new_unchecked("b/c"), "retain").with_retain();

    // client_0 receives retain message upon subscribing to "a/b"
    assert_eq!(timeout(client_0.recv()).await.unwrap(), msg);

    client_1
        .send(MqttRequest::Subscribe(SubscriptionRequest {
            diff: SubscriptionDiff {
                subscribe: ["a/b".into(), "b/c".into()].into(),
                unsubscribe: [].into(),
            },
            client_id: client_id_1,
        }))
        .await
        .unwrap();

    // client_1 should receive both "a/b" and "b/c" retain messages upon subscribing
    let recv = timeout(client_1.recv()).await.unwrap();
    let recv2 = timeout(client_1.recv()).await.unwrap();

    // Retain message should not be redelivered to client_0
    assert!(
        tokio::time::timeout(Duration::from_millis(200), client_0.recv())
            .await
            .is_err()
    );

    if recv.topic.name == "a/b" {
        assert_eq!(recv, msg);
        assert_eq!(recv2, msg2);
    } else {
        assert_eq!(recv, msg2);
        assert_eq!(recv2, msg);
    }
}

#[tokio::test]
async fn dynamic_subscribers_receive_retain_messages_when_upgrading_topic() {
    let broker = mqtt_tests::test_mqtt_broker();
    let mqtt_config = MqttConfig::default()
        .with_port(broker.port)
        .with_session_name("test");
    let mut mqtt = MqttActorBuilder::new(mqtt_config);

    broker
        .publish_with_opts("a/b", "retain", QoS::AtLeastOnce, true)
        .await
        .unwrap();

    let mut client_0 = SimpleMessageBoxBuilder::<_, MqttRequest>::new("dyn-subscriber", 16);
    let mut client_1 = SimpleMessageBoxBuilder::<_, MqttRequest>::new("dyn-subscriber1", 16);
    let _client_id_0 = mqtt.connect_id_sink(TopicFilter::new_unchecked("a/b"), &client_0);
    let client_id_1 = mqtt.connect_id_sink(TopicFilter::empty(), &client_1);
    client_0.connect_sink(NoConfig, &mqtt);
    client_1.connect_sink(NoConfig, &mqtt);
    let mqtt = mqtt.build();
    tokio::spawn(async move { mqtt.run().await.unwrap() });
    let mut client_0 = client_0.build();
    let mut client_1 = client_1.build();

    let msg = MqttMessage::new(&Topic::new_unchecked("a/b"), "retain").with_retain();

    // client_0 receives retain message upon subscribing to "a/b"
    assert_eq!(timeout(client_0.recv()).await.unwrap(), msg);

    client_1
        .send(MqttRequest::Subscribe(SubscriptionRequest {
            diff: SubscriptionDiff {
                subscribe: ["a/+".into()].into(),
                unsubscribe: [].into(),
            },
            client_id: client_id_1,
        }))
        .await
        .unwrap();

    // client_1 should receive "a/b" retain message upon subscribing to a/+
    let recv = timeout(client_1.recv()).await.unwrap();
    assert_eq!(recv, msg);

    // Retain message might be redelivered to client_0, but this is
    // implementation dependent, don't assert either way
}

#[tokio::test]
async fn actor_retrieves_retain_messages_and_sends_them_back_on_provided_sender() {
    let broker = mqtt_tests::test_mqtt_broker();
    let mqtt_config = MqttConfig::default()
        .with_port(broker.port)
        .with_session_name("test");
    let mut mqtt = MqttActorBuilder::new(mqtt_config);

    broker
        .publish_with_opts("a/b", "retain", QoS::AtLeastOnce, true)
        .await
        .unwrap();

    let mut client_0 = SimpleMessageBoxBuilder::<_, MqttRequest>::new("dyn-subscriber", 16);
    let _client_id_0 = mqtt.connect_id_sink(TopicFilter::empty(), &client_0);
    client_0.connect_sink(NoConfig, &mqtt);

    let msg = MqttMessage::new(&Topic::new_unchecked("a/b"), "retain").with_retain();
    let mqtt = mqtt.build();
    tokio::spawn(async move { mqtt.run().await.unwrap() });
    tokio::spawn(async move {
        loop {
            broker.publish("a/b", "non retain").await.unwrap();
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    });
    let mut client_0 = client_0.build();
    let (tx, mut rx) = mpsc::unbounded();
    client_0
        .send(MqttRequest::RetrieveRetain(
            tx,
            TopicFilter::new_unchecked("a/b"),
        ))
        .await
        .unwrap();

    assert_eq!(rx.next().await, Some(msg));
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), rx.next()).await,
        Ok(None)
    );
}

#[tokio::test]
async fn retrieve_retain_channel_closes_if_no_messages_are_sent() {
    let broker = mqtt_tests::test_mqtt_broker();
    let mqtt_config = MqttConfig::default()
        .with_port(broker.port)
        .with_session_name("test");
    let mut mqtt = MqttActorBuilder::new(mqtt_config);

    let mut client_0 = SimpleMessageBoxBuilder::<_, MqttRequest>::new("dyn-subscriber", 16);
    let _client_id_0 = mqtt.connect_id_sink(TopicFilter::empty(), &client_0);
    client_0.connect_sink(NoConfig, &mqtt);

    let mqtt = mqtt.build();
    tokio::spawn(async move { mqtt.run().await.unwrap() });
    let mut client_0 = client_0.build();
    let (tx, mut rx) = mpsc::unbounded();
    client_0
        .send(MqttRequest::RetrieveRetain(
            tx,
            TopicFilter::new_unchecked("a/b"),
        ))
        .await
        .unwrap();

    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), rx.next()).await,
        Ok(None)
    );
}

#[tokio::test]
async fn retrieve_retain_does_not_forward_last_will() {
    let broker = mqtt_tests::test_mqtt_broker();
    let mut last_will_topic = broker.messages_published_on("last-will").await;

    let mqtt_config = MqttConfig::default()
        .with_port(broker.port)
        .with_session_name("test")
        .with_last_will_message(MqttMessage {
            topic: Topic::new_unchecked("last-will"),
            payload: "should not be sent".to_owned().into(),
            qos: QoS::AtLeastOnce,
            retain: true,
        });
    let mut mqtt = MqttActorBuilder::new(mqtt_config);

    let mut client_0 = SimpleMessageBoxBuilder::<_, MqttRequest>::new("dyn-subscriber", 16);
    let _client_id_0 = mqtt.connect_id_sink(TopicFilter::empty(), &client_0);
    client_0.connect_sink(NoConfig, &mqtt);

    let mqtt = mqtt.build();
    tokio::spawn(async move { mqtt.run().await.unwrap() });
    let mut client_0 = client_0.build();
    let (tx, mut rx) = mpsc::unbounded();
    client_0
        .send(MqttRequest::RetrieveRetain(
            tx,
            TopicFilter::new_unchecked("a/b"),
        ))
        .await
        .unwrap();

    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), rx.next()).await,
        Ok(None)
    );
    assert!(
        last_will_topic.try_next().is_err(),
        "Last will should not be published by retrieve retain"
    );
}

async fn timeout<T>(fut: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(1), fut)
        .await
        .expect("Timed out")
}

async fn mqtt_actor(builder: MqttActorBuilder) {
    let mqtt_actor = builder.build();
    mqtt_actor.run().await.unwrap()
}
