#[cfg(test)]
mod tests {
    use crate::*;
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test]
    async fn subscribing_to_many_topics() -> Result<(), anyhow::Error> {
        // Given an MQTT broker
        let broker = mqtt_tests::test_mqtt_broker();

        // A client can subscribe to many topics
        let mut topics = TopicFilter::new("/a/first/topic")?;
        topics.add("/a/second/topic")?;
        topics.add("/a/+/pattern")?; // one can use + pattern
        topics.add("/any/#")?; // one can use # pattern

        let config = Config::default().with_port(broker.port);
        let con = Connection::connect(
            "client_subscribing_to_many_topics",
            &config,
            topics,
        ).await?;

        // The messages for these topics will all be received on the same message stream
        let mut messages = con.received;

        // A message published on any of the subscribed topics must be received
        for (topic_name, payload) in vec![
            ("/a/first/topic", "a first message"),
            ("/a/second/topic", "a second message"),
            ("/a/plus/pattern", "a third message"),
            ("/any/sub/topic", "a fourth message"),
        ]
            .into_iter()
        {
            let topic = Topic::new(topic_name)?;
            let () = broker.publish(topic_name, payload).await?;

            tokio::select! {
                maybe_msg = messages.recv() => {
                    let msg = maybe_msg.expect("Unexpected end of stream");
                    assert_eq!(msg.topic, topic);
                    assert_eq!(msg.payload_str()?, payload);
                }
                _ = sleep(Duration::from_millis(1000)) => {
                    assert!(false, "No message received after a second");
                }
            }
        }

        // No message should be received from un-subscribed topics
        for (topic, payload) in vec![
            ("/a/third/topic", "unrelated message"),
            ("/unrelated/topic", "unrelated message"),
        ]
            .into_iter()
        {
            let () = broker.publish(topic, payload).await?;

            tokio::select! {
                _ = messages.recv() => {
                    assert!(false, "Unrelated message received");
                }
                _ = sleep(Duration::from_millis(1000)) => {
                }
            }
        }

        Ok(())
    }
}
