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
}
