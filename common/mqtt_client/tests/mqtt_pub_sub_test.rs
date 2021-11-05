use mqtt_client::{Client, Message, MqttClient, Topic, TopicFilter};
use std::time::Duration;
use tokio::time::sleep;

#[test]
fn sending_and_receiving_a_message() {
    async fn scenario(payload: String) -> Result<Option<Message>, mqtt_client::MqttClientError> {
        let broker = mqtt_tests::test_mqtt_broker();
        let topic = Topic::new("test/uubpb9wyi9asi46l624f")?;
        let subscriber = Client::connect(
            "subscribe",
            &mqtt_client::Config::default().with_port(broker.port),
        )
        .await?;
        let mut received = subscriber.subscribe(topic.filter()).await?;
        sleep(Duration::from_millis(1000)).await;

        let message = Message::new(&topic, payload);
        let publisher = Client::connect(
            "publisher",
            &mqtt_client::Config::default().with_port(broker.port),
        )
        .await?;
        let () = publisher.publish(message).await?;

        tokio::select! {
            msg = received.next() => Ok(msg),
            _ = sleep(Duration::from_millis(1000)) => Ok(None)
        }
    }

    let payload = String::from("Hello there!");
    match tokio_test::block_on(scenario(payload.clone())) {
        Ok(Some(rcv_message)) => assert_eq!(rcv_message.payload_str().unwrap(), payload),
        Ok(None) => panic!("Got no message after 1s"),
        Err(e) => panic!("Got an error: {}", e),
    }
}

#[tokio::test]
async fn subscribing_to_many_topics() -> Result<(), anyhow::Error> {
    // Given an MQTT broker
    let broker = mqtt_tests::test_mqtt_broker();

    // And an MQTT client connected to that server
    let subscriber = Client::connect(
        "client_subscribing_to_many_topics",
        &mqtt_client::Config::default().with_port(broker.port),
    )
    .await?;

    // The client can subscribe to many topics
    let mut topic_filter = TopicFilter::new("/a/first/topic")?;
    topic_filter.add("/a/second/topic")?;
    topic_filter.add("/a/+/pattern")?; // one can use + pattern
    topic_filter.add("/any/#")?; // one can use # pattern

    // The messages for these topics will all be received on the same message stream
    let mut messages = subscriber.subscribe(topic_filter).await?;
    sleep(Duration::from_millis(1000)).await;

    // So let us create another MQTT client publishing messages.
    let publisher = Client::connect(
        "client_publishing_to_many_topics",
        &mqtt_client::Config::default().with_port(broker.port),
    )
    .await?;

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
        let message = Message::new(&topic, payload);
        let () = publisher.publish(message).await?;

        tokio::select! {
            maybe_msg = messages.next() => {
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
    for (topic_name, payload) in vec![
        ("/a/third/topic", "unrelated message"),
        ("/unrelated/topic", "unrelated message"),
    ]
    .into_iter()
    {
        let topic = Topic::new(topic_name)?;
        let message = Message::new(&topic, payload);
        let () = publisher.publish(message).await?;

        tokio::select! {
            _ = messages.next() => {
                assert!(false, "Unrelated message received");
            }
            _ = sleep(Duration::from_millis(1000)) => {
            }
        }
    }

    Ok(())
}
