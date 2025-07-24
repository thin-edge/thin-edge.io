use crate::*;
use futures::SinkExt;
use futures::StreamExt;
use std::convert::TryInto;
use std::time::Duration;
use tokio::time::timeout;

const TIMEOUT: Duration = Duration::from_millis(1000);

/// Prefixes a topic/session name with a module path and line number
///
/// This allows multiple tests to share an MQTT broker, allowing them to
/// run concurrently within a single test process.
macro_rules! uniquify {
    ($name:literal) => {
        ::std::concat!(::std::module_path!(), ::std::line!(), "-", $name)
    };
}

#[tokio::test]
async fn subscribing_to_messages() {
    // Given an MQTT broker
    let broker = mqtt_tests::test_mqtt_broker();
    let mqtt_config = Config::default().with_port(broker.port);

    // A client subscribes to a topic on connect
    let topic = uniquify!("a/test/topic");
    let mqtt_config = mqtt_config
        .with_session_name(uniquify!("test_client"))
        .with_subscriptions(topic.try_into().unwrap());
    let mut con = Connection::new(&mqtt_config).await.unwrap();

    // Any messages published on that topic ...
    broker.publish(topic, "msg 1").await.unwrap();
    broker.publish(topic, "msg 2").await.unwrap();
    broker.publish(topic, "msg 3").await.unwrap();

    // ... must be received by the client
    assert_eq!(
        MaybeMessage::Next(message(topic, "msg 1")),
        next_message(&mut con.received).await
    );
    assert_eq!(
        MaybeMessage::Next(message(topic, "msg 2")),
        next_message(&mut con.received).await
    );
    assert_eq!(
        MaybeMessage::Next(message(topic, "msg 3")),
        next_message(&mut con.received).await
    );
}

#[derive(Debug, Clone, Eq, PartialEq)]
enum MaybeMessage {
    Next(MqttMessage),
    Eos,
    Timeout,
}

fn message(t: &str, p: &str) -> MqttMessage {
    let topic = Topic::new(t).expect("a valid topic");
    let payload = p.as_bytes();
    MqttMessage::new(&topic, payload)
}

async fn next_message(received: &mut (impl StreamExt<Item = MqttMessage> + Unpin)) -> MaybeMessage {
    match tokio::time::timeout(TIMEOUT, received.next()).await {
        Ok(Some(msg)) => MaybeMessage::Next(msg),
        Ok(None) => MaybeMessage::Eos,
        Err(_elapsed) => MaybeMessage::Timeout,
    }
}

#[tokio::test]
async fn subscribing_to_many_topics() {
    // Given an MQTT broker
    let broker = mqtt_tests::test_mqtt_broker();
    let mqtt_config = Config::default().with_port(broker.port);

    // A client can subscribe to many topics
    let topics = vec![
        "many_topics/a/first/topic",
        "many_topics/a/second/topic",
        "many_topics/a/+/pattern", // one can use + pattern
        "many_topics/any/#",       // one can use # pattern
    ]
    .try_into()
    .expect("a list of topic filters");

    let mqtt_config = mqtt_config
        .with_session_name(uniquify!("client_subscribing_to_many_topics"))
        .with_subscriptions(topics);
    let con = Connection::new(&mqtt_config).await.unwrap();

    // The messages for these topics will all be received on the same message stream
    let mut messages = con.received;

    // A message published on any of the subscribed topics must be received
    for (topic, payload) in vec![
        ("many_topics/a/first/topic", "a first message"),
        ("many_topics/a/second/topic", "a second message"),
        ("many_topics/a/plus/pattern", "a third message"),
        ("many_topics/any/sub/topic", "a fourth message"),
    ]
    .into_iter()
    {
        broker.publish(topic, payload).await.unwrap();
        assert_eq!(
            MaybeMessage::Next(message(topic, payload)),
            next_message(&mut messages).await
        );
    }

    // No message should be received from un-subscribed topics
    for (topic, payload) in vec![
        ("many_topics/a/third/topic", "unrelated message"),
        ("many_topics/unrelated/topic", "unrelated message"),
    ]
    .into_iter()
    {
        broker.publish(topic, payload).await.unwrap();
        assert_eq!(MaybeMessage::Timeout, next_message(&mut messages).await);
    }
}

#[tokio::test]
async fn publishing_messages() -> Result<(), anyhow::Error> {
    // Given an MQTT broker
    let broker = mqtt_tests::test_mqtt_broker();
    let mqtt_config = Config::default().with_port(broker.port);

    let topic = uniquify!("foo/topic");
    let mut all_messages = broker.messages_published_on(topic).await;

    // A client that wish only publish messages doesn't have to subscribe to any topics
    let mqtt_config = mqtt_config.with_session_name(uniquify!("publishing_messages"));
    let mut con = Connection::new(&mqtt_config).await?;

    // Then all messages produced on the `con.published` channel
    con.published.send(message(topic, "foo payload")).await?;
    con.published
        .send(message(topic, "again a foo payload"))
        .await?;
    con.published.send(message(topic, "bar payload")).await?;

    // ... must be actually published
    mqtt_tests::assert_received(
        &mut all_messages,
        TIMEOUT,
        vec!["foo payload", "again a foo payload", "bar payload"],
    )
    .await;

    Ok(())
}

#[tokio::test]
async fn implementing_a_message_mapper() -> Result<(), anyhow::Error> {
    // Given an MQTT broker
    let broker = mqtt_tests::test_mqtt_broker();
    let mqtt_config = Config::default().with_port(broker.port);

    // and an MQTT connection with input and output topics
    let in_topic = uniquify!("mapper/input");
    let out_topic = uniquify!("mapper/output");
    let mut out_messages = broker.messages_published_on(out_topic).await;

    let mqtt_config = mqtt_config
        .with_session_name(uniquify!("mapper"))
        .with_subscriptions(in_topic.try_into().expect("a valid topic filter"));
    let con = Connection::new(&mqtt_config).await?;

    // A message mapper can be implemented as
    // * a consumer of input messages
    // * and a producer of output messages
    // * unaware of the underlying MQTT connection.
    let mut input = con.received;
    let mut output = con.published;
    tokio::spawn(async move {
        while let MaybeMessage::Next(msg) = next_message(&mut input).await {
            let req = msg.payload_str().expect("utf8 payload");
            let res = req.to_uppercase();
            let msg = message(out_topic, &res);
            if output.send(msg).await.is_err() {
                // the connection has been closed
                break;
            }
        }
    });

    // Any messages published on the input topic ...
    broker.publish(in_topic, "msg 1").await?;
    broker.publish(in_topic, "msg 2").await?;
    broker.publish(in_topic, "msg 3").await?;

    // ... is then transformed and published on the output topic.
    mqtt_tests::assert_received(&mut out_messages, TIMEOUT, vec!["MSG 1", "MSG 2", "MSG 3"]).await;

    Ok(())
}

#[tokio::test]
async fn receiving_messages_while_not_connected() -> Result<(), anyhow::Error> {
    // Given an MQTT broker
    let broker = mqtt_tests::test_mqtt_broker();
    let mqtt_config = Config::default().with_port(broker.port);

    // A client that connects with a well-known session name, subscribing to some topic.
    let session_name = "remember_me";
    let topic = uniquify!("my/nice/topic");
    let mqtt_config = mqtt_config
        .with_session_name(session_name)
        .with_subscriptions(topic.try_into()?);
    {
        let con = Connection::new(&mqtt_config).await?;
        con.close().await;
    }

    // Any messages published on that topic while down ...
    broker.publish(topic, "1st msg sent when down").await?;
    broker.publish(topic, "2nd msg sent when down").await?;
    broker.publish(topic, "3rd msg sent when down").await?;

    // ... will be received by the client once back with the same session name
    let mut con = Connection::new(&mqtt_config).await?;

    assert_eq!(
        MaybeMessage::Next(message(topic, "1st msg sent when down")),
        next_message(&mut con.received).await
    );
    assert_eq!(
        MaybeMessage::Next(message(topic, "2nd msg sent when down")),
        next_message(&mut con.received).await
    );
    assert_eq!(
        MaybeMessage::Next(message(topic, "3rd msg sent when down")),
        next_message(&mut con.received).await
    );

    Ok(())
}

#[tokio::test]
async fn testing_an_mqtt_client_without_mqtt() -> Result<(), anyhow::Error> {
    static OUT_TOPIC: &str = uniquify!("out/topic");
    static IN_TOPIC: &str = uniquify!("in/topic");
    // Given an mqtt client
    async fn run(mut input: impl SubChannel, mut output: impl PubChannel) {
        while let Some(msg) = input.next().await {
            let req = msg.payload_str().expect("utf8 payload");
            let res = req.to_uppercase();
            let msg = message(OUT_TOPIC, &res);
            if output.send(msg).await.is_err() {
                break;
            }
        }
        let _ = output.close().await;
    }

    // This client can be tested without any MQTT broker.
    let input = vec![
        message(IN_TOPIC, "a message"),
        message(IN_TOPIC, "another message"),
        message(IN_TOPIC, "yet another message"),
    ];
    let expected = vec![
        message(OUT_TOPIC, "A MESSAGE"),
        message(OUT_TOPIC, "ANOTHER MESSAGE"),
        message(OUT_TOPIC, "YET ANOTHER MESSAGE"),
    ];

    let input_stream = mqtt_tests::input_stream(input).await;
    let (output, output_sink) = mqtt_tests::output_stream();
    tokio::spawn(async move { run(input_stream, output_sink).await });
    assert_eq!(expected, output.collect().await);

    // This very same client can be tested with an MQTT broker
    let broker = mqtt_tests::test_mqtt_broker();
    let mqtt_config = Config::default().with_port(broker.port);
    let mut out_messages = broker.messages_published_on(OUT_TOPIC).await;

    let in_topic = IN_TOPIC.try_into().expect("a valid topic filter");
    let mqtt_config = mqtt_config
        .with_session_name("mapper under test")
        .with_subscriptions(in_topic);
    let con = Connection::new(&mqtt_config).await?;
    tokio::spawn(async move { run(con.received, con.published).await });

    broker.publish(IN_TOPIC, "msg 1, over MQTT").await?;
    broker.publish(IN_TOPIC, "msg 2, over MQTT").await?;
    broker.publish(IN_TOPIC, "msg 3, over MQTT").await?;

    mqtt_tests::assert_received(
        &mut out_messages,
        TIMEOUT,
        vec!["MSG 1, OVER MQTT", "MSG 2, OVER MQTT", "MSG 3, OVER MQTT"],
    )
    .await;

    Ok(())
}

#[tokio::test]
async fn ensure_that_all_messages_are_sent_before_disconnect() -> Result<(), anyhow::Error> {
    let broker = mqtt_tests::test_mqtt_broker();
    let topic = uniquify!("data/topic");
    let mut messages = broker.messages_published_on(topic).await;

    // An mqtt process publishing messages
    // must ensure the messages have been sent before process exit.
    let mqtt_config = Config::default().with_port(broker.port);

    let topic = Topic::new_unchecked(topic);
    let mut con = Connection::new(&mqtt_config).await.expect("a connection");

    con.published
        .send(MqttMessage::new(&topic, "datum 1"))
        .await
        .expect("message sent");
    con.published
        .send(MqttMessage::new(&topic, "datum 2"))
        .await
        .expect("message sent");
    con.published
        .send(MqttMessage::new(&topic, "datum 3"))
        .await
        .expect("message sent");

    // Wait for all the messages to be actually sent
    // before the runtime is shutdown dropping the mqtt sender loop.
    tokio::time::timeout(Duration::from_secs(5), con.close())
        .await
        .expect("MQTT channel should close");

    mqtt_tests::assert_received(
        &mut messages,
        TIMEOUT,
        vec!["datum 1", "datum 2", "datum 3"],
    )
    .await;

    Ok(())
}

#[tokio::test]
async fn ensure_that_last_will_message_is_delivered() -> Result<(), anyhow::Error> {
    let topic = uniquify!("test/lwp");
    let broker = mqtt_tests::test_mqtt_broker();
    // start a subscriber to capture all the messages
    let mut messages = broker.messages_published_on(topic).await;

    // An mqtt client with last will message, publishing messages
    // must ensure the messages have been sent before process exit.
    tokio::spawn(async move {
        let topic = Topic::new_unchecked(topic);
        let mqtt_config = Config::default()
            .with_port(broker.port)
            .with_last_will_message(MqttMessage {
                topic: topic.clone(),
                payload: "good bye".to_string().into(),
                qos: QoS::AtLeastOnce,
                retain: false,
            });
        let mut con = Connection::new(&mqtt_config).await.expect("a connection");

        con.published
            .send(MqttMessage::new(&topic, "hello 1"))
            .await
            .expect("message sent");

        con.published
            .send(MqttMessage::new(&topic, "hello 2"))
            .await
            .expect("message sent");

        con.published
            .send(MqttMessage::new(&topic, "hello 3"))
            .await
            .expect("message sent");

        con.close().await;
    });

    mqtt_tests::assert_received(
        &mut messages,
        Duration::from_secs(3),
        vec!["hello 1", "hello 2", "hello 3", "good bye"],
    )
    .await;
    Ok(())
}

#[tokio::test]
async fn test_retain_message_delivery() -> Result<(), anyhow::Error> {
    // Given an MQTT broker
    let broker = mqtt_tests::test_mqtt_broker();
    let mqtt_config = Config::default().with_port(broker.port);

    let topic = uniquify!("retained/topic");
    let mqtt_config = mqtt_config.with_subscriptions(topic.try_into()?);

    // A client that subscribes to a topic.
    let mut first_subscriber = Connection::new(&mqtt_config).await?;

    //Raise retained alarm message
    broker
        .publish_with_opts(topic, "a retained message", QoS::AtLeastOnce, true)
        .await
        .unwrap();

    //Expect the non-empty retained message to be delivered to first_subscriber
    assert_eq!(
        MaybeMessage::Next(message(topic, "a retained message")),
        next_message(&mut first_subscriber.received).await
    );

    //Clear the last raised retained message
    broker
        .publish_with_opts(
            topic,
            "", //Empty message indicates clear
            QoS::AtLeastOnce,
            true,
        )
        .await
        .unwrap();

    // Connect to the broker with the same session id
    let mut second_subscriber = Connection::new(&mqtt_config).await?;

    //Expect no messages to be delivered to this second_subscriber as the retained message is already cleared
    assert_eq!(
        MaybeMessage::Timeout,
        next_message(&mut second_subscriber.received).await
    );

    //Expect the empty retained message to be delivered to first_subscriber
    assert_eq!(
        MaybeMessage::Next(message(topic, "")),
        next_message(&mut first_subscriber.received).await
    );

    Ok(())
}

#[tokio::test]
async fn test_max_packet_size_validation() -> Result<(), anyhow::Error> {
    // Given an MQTT broker
    let broker = mqtt_tests::test_mqtt_broker();
    let mqtt_config = Config::default()
        .with_port(broker.port)
        .with_max_packet_size(4);

    // A client subscribes to a topic on connect
    let topic = uniquify!("a/test/topic");
    let mqtt_config = mqtt_config
        .with_session_name(uniquify!("test_client"))
        .with_subscriptions(topic.try_into()?);
    let mut con = Connection::new(&mqtt_config).await?;

    // Any messages published on that topic ...
    broker.publish(topic, "aa").await?;
    broker.publish(topic, "aaaaa").await?; // 5 bytes, exceeding max packet size of 4
    broker.publish(topic, "aaa").await?;

    // ... must be received by the client
    assert_eq!(
        MaybeMessage::Next(message(topic, "aa")),
        next_message(&mut con.received).await
    );
    assert_eq!(
        MaybeMessage::Next(message(topic, "aaa")),
        next_message(&mut con.received).await
    );

    Ok(())
}

#[tokio::test]
async fn dynamic_subscriptions() {
    // Given an MQTT broker
    let broker = mqtt_tests::test_mqtt_broker();
    let mqtt_config = Config::default().with_port(broker.port);

    // A client subscribes to a topic on connect
    let topic = uniquify!("a/test/topic");
    let topic2 = uniquify!("a/test/topic/2");

    // Publish a retain message before any client connects
    broker
        .publish_with_opts(topic2, "msg 0", QoS::AtLeastOnce, true)
        .await
        .unwrap();

    let mqtt_config = mqtt_config.with_session_name(uniquify!("test_client"));
    let mut con = Connection::new(&mqtt_config).await.unwrap();

    assert_eq!(
        *con.subscriptions.subscriptions.lock().unwrap(),
        TopicFilter::empty()
    );
    con.subscriptions
        .subscribe_many([topic.to_owned()])
        .await
        .unwrap();

    // Assert we have added the newly subscribed topic to the list we need to
    // resubscribe to if the connection drops
    assert_eq!(
        *con.subscriptions.subscriptions.lock().unwrap(),
        TopicFilter::new_unchecked(topic)
    );

    broker
        .publish_with_opts(topic, "msg 1", QoS::AtLeastOnce, true)
        .await
        .unwrap();

    // Assert just against the payload since the retain flag may be true or
    // false depending on the ordering of the publish/subscribe messages (retain
    // flag is only set on an incoming message if the message was published
    // before we subscribe)
    assert_payload_received(&mut con, "msg 1").await;

    // Unsubscribe from one topic and subscribe to another
    con.subscriptions
        .unsubscribe_many([topic.to_owned()])
        .await
        .unwrap();
    con.subscriptions
        .subscribe_many([topic2.to_owned()])
        .await
        .unwrap();

    // Check we've updated the subscription list with both those changes
    assert_eq!(
        *con.subscriptions.subscriptions.lock().unwrap(),
        TopicFilter::new_unchecked(topic2)
    );

    // We expect now to receive the retained message that has been published before the connection created
    // on the topic2 which the connection only subscribed to now.
    assert_payload_received(&mut con, "msg 0").await;

    // Wait for the new subscription to be enacted
    broker
        .publish_with_opts(topic2, "msg 2", QoS::AtLeastOnce, true)
        .await
        .unwrap();

    assert_payload_received(&mut con, "msg 2").await;

    // At this point, we are unsubscribed from topic, and therefore we shouldn't receive the messages
    broker
        .publish_with_opts(topic, "msg 3", QoS::AtLeastOnce, true)
        .await
        .unwrap();
    broker
        .publish_with_opts(topic2, "msg 4", QoS::AtLeastOnce, true)
        .await
        .unwrap();

    assert_payload_received(&mut con, "msg 4").await;
}

async fn assert_payload_received(con: &mut Connection, payload: &'static str) {
    match next_message(&mut con.received).await {
        MaybeMessage::Next(msg) => assert_eq!(msg.payload.as_str().unwrap(), payload),
        not_msg => panic!("Expected message to be received, got {not_msg:?}"),
    }
}

#[tokio::test]
async fn connections_from_cloned_configs_are_independent() -> Result<(), anyhow::Error> {
    // This test arose from an issue with dynamic subscriptions where
    // subscriptions were shared between different MQTT channel instances
    let broker = mqtt_tests::test_mqtt_broker();
    let mqtt_config = Config::default().with_port(broker.port);
    let mqtt_config_cloned = mqtt_config.clone();

    let topic = uniquify!("a/test/topic");
    let other_topic = uniquify!("different/test/topic");
    let mqtt_config = mqtt_config.with_subscriptions(TopicFilter::new_unchecked(topic));
    let mqtt_config_cloned =
        mqtt_config_cloned.with_subscriptions(TopicFilter::new_unchecked(other_topic));

    let mut con = Connection::new(&mqtt_config).await?;
    let mut other_con = Connection::new(&mqtt_config_cloned).await?;

    // Any messages published on that topic ...
    broker.publish(topic, "original topic message").await?;
    broker.publish(other_topic, "other topic message").await?;

    // ... must be received by the client
    assert_eq!(
        MaybeMessage::Next(message(topic, "original topic message")),
        next_message(&mut con.received).await
    );
    assert_eq!(
        MaybeMessage::Next(message(other_topic, "other topic message")),
        next_message(&mut other_con.received).await
    );

    assert!(
        timeout(Duration::from_millis(200), next_message(&mut con.received))
            .await
            .is_err()
    );
    assert!(timeout(
        Duration::from_millis(200),
        next_message(&mut other_con.received)
    )
    .await
    .is_err());

    Ok(())
}

#[tokio::test]
async fn connection_can_be_closed_after_last_will_published() -> Result<(), anyhow::Error> {
    // This test arose from an issue with dynamic subscriptions where
    // subscriptions were shared between different MQTT channel instances
    let broker = mqtt_tests::test_mqtt_broker();
    let last_will_topic = uniquify!("last/will");

    let mqtt_config = Config::default()
        .with_port(broker.port)
        .with_last_will_message(MqttMessage {
            topic: Topic::new_unchecked(last_will_topic),
            payload: "test".to_owned().into(),
            qos: QoS::AtLeastOnce,
            retain: true,
        });

    let con = Connection::new(&mqtt_config).await?;

    timeout(Duration::from_secs(1), con.close())
        .await
        .expect("Connection did not close");

    Ok(())
}
