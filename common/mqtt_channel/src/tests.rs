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
        let config = Config::default().with_port(broker.port);

        // A client subscribes to a topic on connect
        let topic = "test/topic";
        let mut con = Connection::connect("test_client", &config, topic.try_into()?).await?;
        //sleep(TIMEOUT).await;

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

        // A client can subscribe to many topics
        let mut topics = TopicFilter::new("/a/first/topic")?;
        topics.add("/a/second/topic")?;
        topics.add("/a/+/pattern")?; // one can use + pattern
        topics.add("/any/#")?; // one can use # pattern

        let config = Config::default().with_port(broker.port);
        let con = Connection::connect("client_subscribing_to_many_topics", &config, topics).await?;

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
}
