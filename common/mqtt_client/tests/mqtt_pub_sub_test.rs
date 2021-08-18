use mqtt_client::{Client, Message, MqttClient, QoS, Topic, TopicFilter};
use std::time::Duration;
use tests_mqtt_server::TestsMqttServer;
use tokio::time::sleep;

const MQTT_TEST_PORT: u16 = 55555;

#[tokio::test]
#[cfg_attr(not(feature = "requires-mosquitto"), ignore)]
async fn sending_and_receiving_a_message() {
    let _server = TestsMqttServer::new_with_port(MQTT_TEST_PORT);

    let topic = Topic::new("test/uubpb9wyi9asi46l624f").unwrap();
    let subscriber = Client::connect(
        "subscribe",
        &mqtt_client::Config::default().with_port(MQTT_TEST_PORT),
    )
    .await
    .unwrap();
    let mut received = subscriber.subscribe(topic.filter()).await.unwrap();

    let payload = String::from("Hello there!");
    let message = Message::new(&topic, payload.clone()).qos(QoS::ExactlyOnce);
    let publisher = Client::connect(
        "publisher",
        &mqtt_client::Config::default().with_port(MQTT_TEST_PORT),
    )
    .await
    .unwrap();
    let _pkid = publisher.publish(message).await.unwrap();
    publisher.all_completed().await;

    match tokio::time::timeout(Duration::from_millis(1000), received.next()).await {
        Ok(Some(msg)) => assert_eq!(msg.payload_str().unwrap(), payload),
        _ => panic!("Got no message after 1s"),
    };
}

#[tokio::test]
#[cfg_attr(not(feature = "requires-mosquitto"), ignore)]
async fn subscribing_to_many_topics() -> Result<(), anyhow::Error> {
    // Given an MQTT broker
    let _server = TestsMqttServer::new_with_port(MQTT_TEST_PORT);

    // And an MQTT client connected to that server
    let subscriber = Client::connect(
        "client_subscribing_to_many_topics",
        &mqtt_client::Config::default().with_port(MQTT_TEST_PORT),
    )
    .await?;

    // The client can subscribe to many topics
    let mut topic_filter = TopicFilter::new("/a/first/topic")?;
    topic_filter.add("/a/second/topic")?;
    topic_filter.add("/a/+/pattern")?; // one can use + pattern
    topic_filter.add("/any/#")?; // one can use # pattern

    // The messages for these topics will all be received on the same message stream
    let mut messages = subscriber.subscribe(topic_filter).await?;

    // So let us create another MQTT client publishing messages.
    let publisher = Client::connect(
        "client_publishing_to_many_topics",
        &mqtt_client::Config::default().with_port(MQTT_TEST_PORT),
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
        let message = Message::new(&topic, payload).qos(QoS::ExactlyOnce);
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
        let message = Message::new(&topic, payload).qos(QoS::ExactlyOnce);
        let () = publisher.publish(message).await?;
        publisher.all_completed().await;

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
