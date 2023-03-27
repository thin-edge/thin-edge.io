use crate::*;
use futures::SinkExt;
use futures::StreamExt;
use serial_test::serial;
use std::convert::TryInto;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_millis(1000);

#[tokio::test]
#[serial]
async fn subscribing_to_messages() -> Result<(), anyhow::Error> {
    // Given an MQTT broker
    let broker = mqtt_tests::test_mqtt_broker();
    let mqtt_config = Config::default().with_port(broker.port);

    // A client subscribes to a topic on connect
    let topic = "a/test/topic";
    let mqtt_config = mqtt_config
        .with_session_name("test_client")
        .with_subscriptions(topic.try_into()?);
    let mut con = Connection::new(&mqtt_config).await?;

    // Any messages published on that topic ...
    broker.publish(topic, "msg 1").await?;
    broker.publish(topic, "msg 2").await?;
    broker.publish(topic, "msg 3").await?;

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

    Ok(())
}

#[derive(Debug, Clone, Eq, PartialEq)]
enum MaybeMessage {
    Next(Message),
    Eos,
    Timeout,
}

fn message(t: &str, p: &str) -> Message {
    let topic = Topic::new(t).expect("a valid topic");
    let payload = p.as_bytes();
    Message::new(&topic, payload)
}

async fn next_message(received: &mut (impl StreamExt<Item = Message> + Unpin)) -> MaybeMessage {
    match tokio::time::timeout(TIMEOUT, received.next()).await {
        Ok(Some(msg)) => MaybeMessage::Next(msg),
        Ok(None) => MaybeMessage::Eos,
        Err(_elapsed) => MaybeMessage::Timeout,
    }
}

#[tokio::test]
#[serial]
async fn subscribing_to_many_topics() -> Result<(), anyhow::Error> {
    // Given an MQTT broker
    let broker = mqtt_tests::test_mqtt_broker();
    let mqtt_config = Config::default().with_port(broker.port);

    // A client can subscribe to many topics
    let topics = vec![
        "/a/first/topic",
        "/a/second/topic",
        "/a/+/pattern", // one can use + pattern
        "/any/#",       // one can use # pattern
    ]
    .try_into()
    .expect("a list of topic filters");

    let mqtt_config = mqtt_config
        .with_session_name("client_subscribing_to_many_topics")
        .with_subscriptions(topics);
    let con = Connection::new(&mqtt_config).await?;

    // The messages for these topics will all be received on the same message stream
    let mut messages = con.received;

    // A message published on any of the subscribed topics must be received
    for (topic, payload) in vec![
        ("/a/first/topic", "a first message"),
        ("/a/second/topic", "a second message"),
        ("/a/plus/pattern", "a third message"),
        ("/any/sub/topic", "a fourth message"),
    ]
    .into_iter()
    {
        broker.publish(topic, payload).await?;
        assert_eq!(
            MaybeMessage::Next(message(topic, payload)),
            next_message(&mut messages).await
        );
    }

    // No message should be received from un-subscribed topics
    for (topic, payload) in vec![
        ("/a/third/topic", "unrelated message"),
        ("/unrelated/topic", "unrelated message"),
    ]
    .into_iter()
    {
        broker.publish(topic, payload).await?;
        assert_eq!(MaybeMessage::Timeout, next_message(&mut messages).await);
    }

    Ok(())
}

#[tokio::test]
#[serial]
async fn publishing_messages() -> Result<(), anyhow::Error> {
    // Given an MQTT broker
    let broker = mqtt_tests::test_mqtt_broker();
    let mqtt_config = Config::default().with_port(broker.port);

    let mut all_messages = broker.messages_published_on("#").await;

    // A client that wish only publish messages doesn't have to subscribe to any topics
    let mqtt_config = mqtt_config.with_session_name("publishing_messages");
    let mut con = Connection::new(&mqtt_config).await?;

    // Then all messages produced on the `con.published` channel
    con.published
        .send(message("foo/topic", "foo payload"))
        .await?;
    con.published
        .send(message("foo/topic", "again a foo payload"))
        .await?;
    con.published
        .send(message("bar/topic", "bar payload"))
        .await?;

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
#[serial]
async fn implementing_a_message_mapper() -> Result<(), anyhow::Error> {
    // Given an MQTT broker
    let broker = mqtt_tests::test_mqtt_broker();
    let mqtt_config = Config::default().with_port(broker.port);

    // and an MQTT connection with input and output topics
    let in_topic = "mapper/input".try_into().expect("a valid topic filter");
    let out_topic = "mapper/output".try_into().expect("a valid topic name");
    let mut out_messages = broker.messages_published_on("mapper/output").await;

    let mqtt_config = mqtt_config
        .with_session_name("mapper")
        .with_subscriptions(in_topic);
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
            let msg = Message::new(&out_topic, res.as_bytes());
            if output.send(msg).await.is_err() {
                // the connection has been closed
                break;
            }
        }
    });

    // Any messages published on the input topic ...
    broker.publish("mapper/input", "msg 1").await?;
    broker.publish("mapper/input", "msg 2").await?;
    broker.publish("mapper/input", "msg 3").await?;

    // ... is then transformed and published on the output topic.
    mqtt_tests::assert_received(&mut out_messages, TIMEOUT, vec!["MSG 1", "MSG 2", "MSG 3"]).await;

    Ok(())
}

#[tokio::test]
#[serial]
async fn receiving_messages_while_not_connected() -> Result<(), anyhow::Error> {
    // Given an MQTT broker
    let broker = mqtt_tests::test_mqtt_broker();
    let mqtt_config = Config::default().with_port(broker.port);

    // A client that connects with a well-known session name, subscribing to some topic.
    let session_name = "remember_me";
    let topic = "my/nice/topic";
    let mqtt_config = mqtt_config
        .with_session_name(session_name)
        .with_subscriptions(topic.try_into()?);
    {
        let _con = Connection::new(&mqtt_config).await?;

        // A connection is disconnected on drop
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
#[serial]
async fn testing_an_mqtt_client_without_mqtt() -> Result<(), anyhow::Error> {
    // Given an mqtt client
    async fn run(mut input: impl SubChannel, mut output: impl PubChannel) {
        let out_topic = Topic::new_unchecked("out/topic");

        while let Some(msg) = input.next().await {
            let req = msg.payload_str().expect("utf8 payload");
            let res = req.to_uppercase();
            let msg = Message::new(&out_topic, res.as_bytes());
            if output.send(msg).await.is_err() {
                break;
            }
        }
        let _ = output.close().await;
    }

    // This client can be tested without any MQTT broker.
    let input = vec![
        message("in/topic", "a message"),
        message("in/topic", "another message"),
        message("in/topic", "yet another message"),
    ];
    let expected = vec![
        message("out/topic", "A MESSAGE"),
        message("out/topic", "ANOTHER MESSAGE"),
        message("out/topic", "YET ANOTHER MESSAGE"),
    ];

    let input_stream = mqtt_tests::input_stream(input).await;
    let (output, output_sink) = mqtt_tests::output_stream();
    tokio::spawn(async move { run(input_stream, output_sink).await });
    assert_eq!(expected, output.collect().await);

    // This very same client can be tested with an MQTT broker
    let broker = mqtt_tests::test_mqtt_broker();
    let mqtt_config = Config::default().with_port(broker.port);
    let mut out_messages = broker.messages_published_on("out/topic").await;

    let in_topic = "in/topic".try_into().expect("a valid topic filter");
    let mqtt_config = mqtt_config
        .with_session_name("mapper under test")
        .with_subscriptions(in_topic);
    let con = Connection::new(&mqtt_config).await?;
    tokio::spawn(async move { run(con.received, con.published).await });

    broker.publish("in/topic", "msg 1, over MQTT").await?;
    broker.publish("in/topic", "msg 2, over MQTT").await?;
    broker.publish("in/topic", "msg 3, over MQTT").await?;

    mqtt_tests::assert_received(
        &mut out_messages,
        TIMEOUT,
        vec!["MSG 1, OVER MQTT", "MSG 2, OVER MQTT", "MSG 3, OVER MQTT"],
    )
    .await;

    Ok(())
}

#[tokio::test]
#[serial]
async fn creating_a_session() -> Result<(), anyhow::Error> {
    // Given an MQTT broker
    let broker = mqtt_tests::test_mqtt_broker();
    let mqtt_config = Config::default().with_port(broker.port);

    // Given an MQTT config with a well-known session name
    let session_name = "my-session-name";
    let topic = "my/topic";
    let mqtt_config = mqtt_config
        .with_session_name(session_name)
        .with_subscriptions(topic.try_into()?);

    // This config can be created to initialize an MQTT session
    init_session(&mqtt_config).await?;

    // Any messages published on that topic
    broker
        .publish(topic, "1st msg sent before a first connection")
        .await?;
    broker
        .publish(topic, "2nd msg sent before a first connection")
        .await?;
    broker
        .publish(topic, "3rd msg sent before a first connection")
        .await?;

    // Will be received by the client with the same session name even for its first connection
    let mut con = Connection::new(&mqtt_config).await?;

    assert_eq!(
        MaybeMessage::Next(message(topic, "1st msg sent before a first connection")),
        next_message(&mut con.received).await
    );
    assert_eq!(
        MaybeMessage::Next(message(topic, "2nd msg sent before a first connection")),
        next_message(&mut con.received).await
    );
    assert_eq!(
        MaybeMessage::Next(message(topic, "3rd msg sent before a first connection")),
        next_message(&mut con.received).await
    );

    Ok(())
}

#[tokio::test]
#[serial]
async fn a_session_must_have_a_name() {
    let broker = mqtt_tests::test_mqtt_broker();
    let mqtt_config = Config::default().with_port(broker.port);

    let result = init_session(&mqtt_config).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Invalid session"));
}

#[tokio::test]
#[serial]
async fn a_named_session_must_not_set_clean_session() {
    let broker = mqtt_tests::test_mqtt_broker();
    let mqtt_config = Config::default()
        .with_port(broker.port)
        .with_session_name("useless name")
        .with_clean_session(true);

    let result = init_session(&mqtt_config).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Invalid session"));
}

#[tokio::test]
#[serial]
async fn cleaning_a_session() -> Result<(), anyhow::Error> {
    // Given an MQTT broker
    let broker = mqtt_tests::test_mqtt_broker();
    let mqtt_config = Config::default().with_port(broker.port);

    // Given an MQTT config with a well-known session name
    let session_name = "a-session-name";
    let topic = "a/topic";
    let mqtt_config = mqtt_config
        .with_session_name(session_name)
        .with_subscriptions(topic.try_into()?);

    // The session being initialized
    init_session(&mqtt_config).await?;

    // And some messages published
    broker
        .publish(topic, "A fst msg published before clean")
        .await?;
    broker
        .publish(topic, "A 2nd msg published before clean")
        .await?;

    // Then we clean the session
    {
        // One just needs a config with the same session name.
        // Subscriptions can be given - but this not required: any previous subscriptions will be cleared.
        let mqtt_config = Config::default()
            .with_port(broker.port)
            .with_session_name(session_name);
        clear_session(&mqtt_config).await?;
    }

    // And publish more messages
    broker
        .publish(topic, "A 3nd msg published after clean")
        .await?;

    // Then no messages will be received by the client with the same session name
    let mut con = Connection::new(&mqtt_config).await?;

    assert_eq!(MaybeMessage::Timeout, next_message(&mut con.received).await);

    Ok(())
}

#[tokio::test]
#[serial]
async fn to_be_cleared_a_session_must_have_a_name() {
    let broker = mqtt_tests::test_mqtt_broker();
    let mqtt_config = Config::default().with_port(broker.port);

    let result = clear_session(&mqtt_config).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Invalid session"));
}

#[tokio::test]
#[serial]
async fn ensure_that_all_messages_are_sent_before_disconnect() -> Result<(), anyhow::Error> {
    let broker = mqtt_tests::test_mqtt_broker();
    let topic = "data/topic";
    let mut messages = broker.messages_published_on(topic).await;

    // An mqtt process publishing messages
    // must ensure the messages have been sent before process exit.
    std::thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let mqtt_config = Config::default().with_port(broker.port);

                let topic = Topic::new_unchecked(topic);
                let mut con = Connection::new(&mqtt_config).await.expect("a connection");

                con.published
                    .send(Message::new(&topic, "datum 1"))
                    .await
                    .expect("message sent");
                con.published
                    .send(Message::new(&topic, "datum 2"))
                    .await
                    .expect("message sent");
                con.published
                    .send(Message::new(&topic, "datum 3"))
                    .await
                    .expect("message sent");

                // Wait for all the messages to be actually sent
                // before the runtime is shutdown dropping the mqtt sender loop.
                con.close().await;
            });
    });

    mqtt_tests::assert_received(
        &mut messages,
        TIMEOUT,
        vec!["datum 1", "datum 2", "datum 3"],
    )
    .await;

    Ok(())
}

#[tokio::test]
#[serial]
async fn ensure_that_last_will_message_is_delivered() -> Result<(), anyhow::Error> {
    let topic = "test/lwp";
    let broker = mqtt_tests::test_mqtt_broker();
    // start a subscriber to capture all the messages
    let mut messages = broker.messages_published_on(topic).await;

    // An mqtt client with last will message, publishing messages
    // must ensure the messages have been sent before process exit.
    tokio::spawn(async move {
        let topic = Topic::new_unchecked(topic);
        let mqtt_config = Config::default()
            .with_port(broker.port)
            .with_last_will_message(Message {
                topic: topic.clone(),
                payload: "good bye".to_string().into(),
                qos: QoS::AtLeastOnce,
                retain: false,
            });
        let mut con = Connection::new(&mqtt_config).await.expect("a connection");

        con.published
            .send(Message::new(&topic, "hello 1"))
            .await
            .expect("message sent");

        con.published
            .send(Message::new(&topic, "hello 2"))
            .await
            .expect("message sent");

        con.published
            .send(Message::new(&topic, "hello 3"))
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
