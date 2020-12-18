use mqtt_client::{Config, Message, Topic};
use std::time::Duration;
use tokio::time::sleep;

#[test]
fn sending_and_receiving_a_message() {
    async fn scenario(payload: String) -> Result<Option<Message>, mqtt_client::Error> {
        let test_broker = Config {
            host: String::from("test.mosquitto.org"),
            port: 1883,
        };

        let topic = Topic::new("test/uubpb9wyi9asi46l624f")?;
        let subscriber = test_broker.connect("subscriber").await?;
        let mut received = subscriber.subscribe(topic.filter()).await?;

        let message = Message::new(&topic, payload);
        let publisher = test_broker.connect("publisher").await?;
        publisher.publish(message).await?;

        tokio::select! {
            msg = received.next() => Ok(msg),
            _ = sleep(Duration::from_millis(1000)) => Ok(None)
        }
    };

    let payload = String::from("Hello there!");
    match tokio_test::block_on(scenario(payload.clone())) {
        Ok(Some(rcv_message)) => assert_eq!(rcv_message.payload, payload.as_bytes()),
        Ok(None) => panic!("Got no message after 1s"),
        Err(e) => panic!("Got an error: {}", e),
    }
}
