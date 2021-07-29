mod rumqttd_broker;

#[test]
fn sending_and_receiving_a_message() {
    use mqtt_client::{Client, Message, MqttClient, Topic};
    use std::time::Duration;
    use tokio::time::sleep;

    async fn scenario(payload: String) -> Result<Option<Message>, mqtt_client::MqttClientError> {
        let _mqtt_server_handle = tokio::spawn(async {
            rumqttd_broker::start_broker_local("../../configuration/rumqttd/rumqttd_5885.conf")
                .await
        });
        let topic = Topic::new("test/uubpb9wyi9asi46l624f")?;
        let subscriber =
            Client::connect("subscribe", &mqtt_client::Config::default().with_port(5885)).await?;
        let mut received = subscriber.subscribe(topic.filter()).await?;

        let message = Message::new(&topic, payload);
        let publisher =
            Client::connect("publisher", &mqtt_client::Config::default().with_port(5885)).await?;
        let _pkid = publisher.publish(message).await?;

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
