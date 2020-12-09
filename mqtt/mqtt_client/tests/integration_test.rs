use mqtt_client::{Client,Message,Topic};
use std::time::Duration;
use tokio::time::sleep;

#[test]
fn sending_and_receiving_a_message() {
    //broker::start();

    async fn scenario(payload: String) -> Result<Option<Message>, mqtt_client::Error> {
        let topic = Topic::new("test")?;
        let subscriber = Client::connect("subscriber").await?;
        let mut received = subscriber.subscribe(topic.filter()).await?;

        let message = Message::new(&topic, payload);
        let publisher = Client::connect("publisher").await?;
        publisher.publish(message).await?;

        tokio::select! {
            msg = received.next() => Ok(msg),
            _ = sleep(Duration::from_millis(3000)) => Ok(None)
        }
    };

    let payload= String::from("Hello there!");
    match tokio_test::block_on(scenario(payload.clone())) {
        Ok(Some(rcv_message)) => assert_eq!(rcv_message.payload, payload.as_bytes()),
        Ok(None) => panic!("Got no message after 3s"),
        Err(e) => panic!("Got an error: {}", e),
    }
}
/*
mod broker {
    use librumqttd::{Broker,Config};

    pub fn start() {
        let mut broker = Broker::new(Config::default());
        broker.start().unwrap();
    }
}*/
