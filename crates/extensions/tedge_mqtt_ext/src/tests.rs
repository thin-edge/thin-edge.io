use crate::*;
use mqtt_channel::Topic;

#[tokio::test]
async fn communicate_over_mqtt() {
    let broker = mqtt_tests::test_mqtt_broker();
    let mqtt_config = MqttConfig::default().with_port(broker.port);
    let mut mqtt_actor_builder = MqttActorBuilder::new(mqtt_config);

    let alice_topic = Topic::new_unchecked("messages/for/alice");
    let mut alice = mqtt_actor_builder.new_client("Alice", alice_topic.clone().into());

    let bob_topic = Topic::new_unchecked("messages/for/bob");
    let mut bob = mqtt_actor_builder.new_client("Bob", bob_topic.clone().into());

    let mut all_topics = TopicFilter::empty();
    all_topics.add_all(alice_topic.clone().into());
    all_topics.add_all(bob_topic.clone().into());
    let mut spy = mqtt_actor_builder.new_client("Spy", all_topics);

    tokio::spawn(mqtt_actor(mqtt_actor_builder));

    // Message pub on some topic is received by all subscribers
    assert!(alice
        .send(MqttMessage::new(&bob_topic, "Hi Bob"))
        .await
        .is_ok());
    assert_eq!(
        bob.recv().await,
        Some(MqttMessage::new(&bob_topic, "Hi Bob"))
    );
    assert_eq!(
        spy.recv().await,
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
    assert_eq!(bob.recv().await, Some(MqttMessage::new(&bob_topic, "A")));
    assert_eq!(bob.recv().await, Some(MqttMessage::new(&bob_topic, "B")));
    assert_eq!(bob.recv().await, Some(MqttMessage::new(&bob_topic, "C")));

    // When messages are pub/sub on a single topic; they are received in order
    assert_eq!(
        alice.recv().await,
        Some(MqttMessage::new(&alice_topic, "1"))
    );
    assert_eq!(
        alice.recv().await,
        Some(MqttMessage::new(&alice_topic, "2"))
    );
    assert_eq!(
        alice.recv().await,
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

async fn mqtt_actor(builder: MqttActorBuilder) {
    let (mqtt_actor, mqtt_message_box) = builder.build().await;
    mqtt_actor.run(mqtt_message_box).await.unwrap()
}
