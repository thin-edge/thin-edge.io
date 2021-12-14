use mqtt_client::{Client, Message, MqttClient, Topic, TopicFilter};
use std::time::Duration;
use tokio::time::{sleep, timeout};

const TIMEOUT: Duration = Duration::from_millis(1000);

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial]
async fn sending_and_receiving_a_message() {
    // Given a broker and an MQTT message
    let broker = mqtt_tests::test_mqtt_broker();
    let topic = Topic::new("test/uubpb9wyi9asi46l624f").expect("valid topic name");
    let payload = String::from("Hello there!");
    let message = Message::new(&topic, payload.clone());

    // Be ready the receive messages
    let subscriber = Client::connect(
        "subscribe",
        &mqtt_client::Config::default().with_port(broker.port),
    )
    .await
    .expect("subscriber connected to the broker");
    let mut received = subscriber
        .subscribe(topic.filter())
        .await
        .expect("valid topic name");
    sleep(TIMEOUT).await; // because `subscribe()` might return before the sub ack

    // Send a message
    let publisher = Client::connect(
        "publisher",
        &mqtt_client::Config::default().with_port(broker.port),
    )
    .await
    .expect("publisher connected to the broker");
    let () = publisher
        .publish(message)
        .await
        .expect("message to be sent");
    sleep(TIMEOUT).await; // because `publish()` might return before the pub ack

    // Check the message has been received
    match timeout(TIMEOUT, received.next()).await {
        Ok(Some(msg)) => {
            assert_eq!(msg.payload_str().expect("Utf8 payload"), payload)
        }
        Ok(None) => assert!(false, "Unexpected end of stream"),
        Err(_elapsed) => assert!(false, "No message received after a second"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial]
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
    sleep(TIMEOUT).await; // because `subscribe()` might return before the sub ack

    // A message published on any of the subscribed topics must be received
    for (topic_name, payload) in vec![
        ("/a/first/topic", "a first message"),
        ("/a/second/topic", "a second message"),
        ("/a/plus/pattern", "a third message"),
        ("/any/sub/topic", "a fourth message"),
    ]
    .into_iter()
    {
        let () = broker.publish(topic_name, payload).await?;

        match timeout(TIMEOUT, messages.next()).await {
            Ok(Some(msg)) => {
                assert_eq!(&msg.topic.name, topic_name);
                assert_eq!(msg.payload_str().expect("Utf8 payload"), payload)
            }
            Ok(None) => assert!(false, "Unexpected end of stream"),
            Err(_elapsed) => assert!(false, "No message received after a second"),
        }
    }

    // No message should be received from un-subscribed topics
    for (topic_name, payload) in vec![
        ("/a/third/topic", "unrelated message"),
        ("/unrelated/topic", "unrelated message"),
    ]
    .into_iter()
    {
        let () = broker.publish(topic_name, payload).await?;

        match timeout(TIMEOUT, messages.next()).await {
            Ok(Some(_)) => {
                assert!(false, "Unrelated message received");
            }
            Ok(None) | Err(_) => {}
        }
    }

    Ok(())
}
