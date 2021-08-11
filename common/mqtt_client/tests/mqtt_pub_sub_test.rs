mod rumqttd_broker;
use mqtt_client::{Client, Message, MqttClient, Topic, TopicFilter};
use tokio::time::{self, sleep, Duration};

const MQTTTESTPORT: u16 = 58586;

#[test]
fn sending_and_receiving_a_message() {
    async fn scenario(payload: String) -> Result<Option<Message>, mqtt_client::MqttClientError> {
        let mqtt_server_handle =
            tokio::spawn(async { rumqttd_broker::start_broker_local(MQTTTESTPORT).await });
        let topic = Topic::new("test/uubpb9wyi9asi46l624f")?;
        let subscriber = Client::connect(
            "subscribe",
            &mqtt_client::Config::default().with_port(MQTTTESTPORT),
        )
        .await?;
        let mut received = subscriber.subscribe(topic.filter()).await?;

        let message = Message::new(&topic, payload);
        let publisher = Client::connect(
            "publisher",
            &mqtt_client::Config::default().with_port(MQTTTESTPORT),
        )
        .await?;
        let _pkid = publisher.publish(message).await?;
        let sleep = time::sleep(Duration::from_secs(1));
        tokio::pin!(sleep);

        loop {
            tokio::select! {
                msg = received.next() => { mqtt_server_handle.abort(); return Ok(msg);},
                _ = &mut sleep => {mqtt_server_handle.abort(); return Ok(None);}
            }
        }
    }

    let payload = String::from("Hello there!");
    match tokio_test::block_on(scenario(payload.clone())) {
        Ok(Some(rcv_message)) => assert_eq!(rcv_message.payload_str().unwrap(), payload),
        Ok(None) => panic!("Got no message after 3s"),
        Err(e) => panic!("Got an error: {}", e),
    }
}

#[tokio::test]
async fn subscribing_to_many_topics() -> Result<(), anyhow::Error> {
    // Given an MQTT broker
    let mqtt_port: u16 = 55555;
    let mqtt_server_handle =
        tokio::spawn(async move { rumqttd_broker::start_broker_local(mqtt_port).await });

    // And an MQTT client connected to that server
    let subscriber = Client::connect(
        "client_subscribing_to_many_topics",
        &mqtt_client::Config::default().with_port(mqtt_port),
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
        &mqtt_client::Config::default().with_port(mqtt_port),
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

        let sleep = sleep(Duration::from_millis(1000));
        tokio::pin!(sleep);

        loop {
            tokio::select! {
                maybe_msg = messages.next() => {
                    let msg = maybe_msg.expect("Unexpected end of stream");
                    assert_eq!(msg.topic, topic);
                    assert_eq!(msg.payload_str()?, payload);
                    break;
                }
                _ =  &mut sleep  => {
                    assert!(false, "No message received after a second");
                    break;
                }
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
        let sleep = sleep(Duration::from_millis(1000));
        tokio::pin!(sleep);

        loop {
            tokio::select! {
                _ = messages.next() => {
                    assert!(false, "Unrelated message received");
                    break;
                }
                _ =  &mut sleep  => {
                    break;
                }
            }
        }
    }
    mqtt_server_handle.abort();

    Ok(())
}
