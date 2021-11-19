#[cfg(test)]
mod tests {
    use crate::*;
    use serial_test::serial;
    use std::convert::TryInto;
    use std::time::Duration;
    use Stream::*;

    const TIMEOUT: Duration = Duration::from_millis(1000);

    #[tokio::test]
    #[serial]
    async fn subscribing_to_messages() -> Result<(), anyhow::Error> {
        // Given an MQTT broker
        let broker = mqtt_tests::test_mqtt_broker();
        let mqtt_config = Config::default().with_port(broker.port);

        // A client subscribes to a topic on connect
        let topic = "test/topic";
        let mut con = Connection::connect("test_client", &mqtt_config, topic.try_into()?).await?;

        // Any messages published on that topic ...
        broker.publish(topic, "msg 1").await?;
        broker.publish(topic, "msg 2").await?;
        broker.publish(topic, "msg 3").await?;

        // ... must be received by the client
        assert_eq!(
            Next(message(topic, "msg 1")),
            next_message(&mut con.received).await
        );
        assert_eq!(
            Next(message(topic, "msg 2")),
            next_message(&mut con.received).await
        );
        assert_eq!(
            Next(message(topic, "msg 3")),
            next_message(&mut con.received).await
        );

        Ok(())
    }

    #[derive(Debug, Clone, Eq, PartialEq)]
    enum Stream {
        Next(Message),
        Eos,
        Timeout,
    }

    fn message(t: &str, p: &str) -> Message {
        let topic = Topic::new(t).expect("a valid topic");
        let payload = p.as_bytes();
        Message::new(&topic, payload)
    }

    async fn next_message(received: &mut async_broadcast::Receiver<Message>) -> Stream {
        match tokio::time::timeout(TIMEOUT, received.recv()).await {
            Ok(Ok(msg)) => Stream::Next(msg),
            Ok(Err(async_broadcast::RecvError)) => Stream::Eos,
            Err(_elapsed) => Stream::Timeout,
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

        let con =
            Connection::connect("client_subscribing_to_many_topics", &mqtt_config, topics).await?;

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
            let () = broker.publish(topic, payload).await?;
            assert_eq!(
                Next(message(topic, payload)),
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
            let () = broker.publish(topic, payload).await?;
            assert_eq!(Timeout, next_message(&mut messages).await);
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
        let topic = vec![]
            .try_into()
            .expect("a list of topics (possibly empty)");
        let con = Connection::connect("publishing_messages", &mqtt_config, topic).await?;

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

        let con = Connection::connect("mapper", &mqtt_config, in_topic).await?;

        // A message mapper can be implemented as
        // * a consumer of input messages
        // * and a producer of output messages
        // * unaware of the underlying MQTT connection.
        let mut input = con.received;
        let output = con.published;
        tokio::spawn(async move {
            while let Next(msg) = next_message(&mut input).await {
                let req = msg.payload_str().expect("utf8 payload");
                let res = req.to_uppercase();
                let msg = Message::new(&out_topic, res.as_bytes());
                if let Err(_) = output.send(msg).await {
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
        mqtt_tests::assert_received(&mut out_messages, TIMEOUT, vec!["MSG 1", "MSG 2", "MSG 3"])
            .await;

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
        let topic = "test/topic";
        {
            let _con = Connection::connect(session_name, &mqtt_config, topic.try_into()?).await?;

            // A connection is disconnected on drop
        }

        // Any messages published on that topic while down ...
        broker.publish(topic, "1st msg sent when down").await?;
        broker.publish(topic, "2nd msg sent when down").await?;
        broker.publish(topic, "3rd msg sent when down").await?;

        // ... will be received by the client once back with the same session name
        let mut con = Connection::connect(session_name, &mqtt_config, topic.try_into()?).await?;

        assert_eq!(
            Next(message(topic, "1st msg sent when down")),
            next_message(&mut con.received).await
        );
        assert_eq!(
            Next(message(topic, "2nd msg sent when down")),
            next_message(&mut con.received).await
        );
        assert_eq!(
            Next(message(topic, "3rd msg sent when down")),
            next_message(&mut con.received).await
        );

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn testing_an_mqtt_client_without_mqtt() -> Result<(), anyhow::Error> {
        // Given an mqtt client
        async fn run(mut input: impl StreamInput<Message>, mut output: impl StreamOutput<Message>) {
            let in_topic = TopicFilter::new_unchecked("in/topic");
            let out_topic = Topic::new_unchecked("out/topic");

            //let mut input = input.filter(|msg| in_topic.accept(msg));

            while let Some(msg) = input.next().await {
                let req = msg.payload_str().expect("utf8 payload");
                let res = req.to_uppercase();
                let msg = Message::new(&out_topic, res.as_bytes());
                if let Err(_) = output.push(msg).await {
                    // the connection has been closed
                    break;
                }
            }
            //output.done();
        }

        // This client can be tested without any MQTT broker.
        let input = vec![
            message("in/topic", "a message"),
            message("in/topic", "another message"),
            message("in/topic", "yet another message"),
            //message("unrelated/topic", "some unrelated message"),
        ];
        let expected = vec![
            message("out/topic", "A MESSAGE"),
            message("out/topic", "ANOTHER MESSAGE"),
            message("out/topic", "YET ANOTHER MESSAGE"),
        ];

        let mut output = StreamRecorder::new();
        let output_stream = output.collector_stream();
        tokio::spawn(async move { run(input, output_stream) });
        assert_eq!(expected, output.collected().await);

        Ok(())
    }
}
