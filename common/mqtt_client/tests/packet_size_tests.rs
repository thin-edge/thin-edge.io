use futures::future::TryFutureExt;
use librumqttd::{async_locallink, Config};
use mqtt_client::{Client, Message, MqttClient, MqttClientError, QoS, Topic, TopicFilter};

use tokio::time::Duration;
#[derive(Debug)]
enum TestJoinError {
    TestMqttClientError(MqttClientError),
    ElapseTime,
}

#[tokio::test]
//#[cfg(feature = "integration-test")]
// This checks the mqtt packets are within the limit or not
async fn packet_size_within_limit() -> anyhow::Result<()> {
    println!("Start the local broker");
    let mqtt_server_handle = tokio::spawn(async { start_broker_local().await });

    println!("Start the subscriber");
    let subscriber = tokio::spawn(async move { subscribe_messages().await });

    println!("Start the publisher and publish 3 messages");
    let publisher = tokio::spawn(async move { publish_messages().await });

    let _ = publisher.await?;
    let _ = subscriber.await?;
    mqtt_server_handle.abort();
    Ok(())
}

#[tokio::test]
//#[cfg(feature = "integration-test")]
// This checks the mqtt packet size that exceeds the limit
async fn packet_size_exceeds_limit() -> anyhow::Result<()> {
    println!("Start the broker");
    let mqtt_server_handle = tokio::spawn(async { start_broker_local().await });

    println!("Start the publisher and publish a message");
    let publish = tokio::spawn(async { publish_big_message().await });

    mqtt_server_handle.abort();
    publish.await?
}

async fn subscribe_errors(pub_client: &Client) -> Result<(), MqttClientError> {
    let mut errors = pub_client.subscribe_errors();
    while let Some(_error) = errors.next().await {
        return Err(mqtt_client::MqttClientError::ConnectionError(
            rumqttc::ConnectionError::Mqtt4Bytes(rumqttc::Error::PayloadTooLong),
        ));
    }
    Ok(())
}

async fn start_broker_local() -> anyhow::Result<()> {
    let config: Config = confy::load_path("../../configuration/rumqttd/rumqttd.conf")?;
    let (mut router, _console, servers, _builder) = async_locallink::construct_broker(config);
    let router = tokio::task::spawn_blocking(move || -> anyhow::Result<()> { Ok(router.start()?) });
    servers.await;
    let _ = router.await;
    Ok(())
}

async fn subscribe_messages() -> Result<(), anyhow::Error> {
    let sub_filter = TopicFilter::new("test/hello")?;
    let client = Client::connect("subscribe", &mqtt_client::Config::default()).await?;
    let mut messages = client.subscribe(sub_filter).await?;
    let mut cnt: i32 = 0;
    while let Some(_message) = messages.next().await {
        if cnt >= 3 {
            break;
        } else {
            cnt += 1;
        }
    }
    println!("Subscriber: Received all messages, test passed");
    assert!(cnt >= 3);
    client.disconnect().await?;
    Ok(())
}

async fn publish_messages() -> Result<(), anyhow::Error> {
    // create a 128MB message
    let buffer = create_packet(134217728);
    let topic = Topic::new("test/hello")?;
    let client = Client::connect("publish_big_data", &mqtt_client::Config::default()).await?;

    let mut cnt: i32 = 0;
    loop {
        let message = Message::new(&topic, buffer.clone()).qos(QoS::AtMostOnce);
        let () = client.publish(message).await?;
        tokio::time::sleep(Duration::from_secs(1)).await;
        if cnt >= 3 {
            break;
        } else {
            cnt += 1;
        }
    }
    client.disconnect().await?;
    Ok(())
}

async fn publish_big_message() -> Result<(), anyhow::Error> {
    // create a 260MB message
    let buffer = create_packet(272629760);

    let topic = Topic::new("test/hello")?;
    let publish_client =
        Client::connect("publish_big_data", &mqtt_client::Config::default()).await?;

    let message = Message::new(&topic, buffer.clone()).qos(QoS::ExactlyOnce);

    let publish_handle = publish_client.publish(message);

    let timeout = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        subscribe_errors(&publish_client).map_err(|e| TestJoinError::TestMqttClientError(e)),
    )
    .map_err(|_e| TestJoinError::ElapseTime);

    let res = tokio::try_join!(
        timeout,
        publish_handle.map_err(|e| TestJoinError::TestMqttClientError(e))
    );

    match res {
        Ok((_first, _second)) => {
            // if packet exceeds return Ok
            return Ok(());
        }
        Err(TestJoinError::ElapseTime) => {
            // timer elapsed, no packet size errors were caught
            anyhow::bail!("Elapsed time packetsize is ok");
        }
        Err(err) => {
            dbg!("processing failed; error = {:?}", err);
            return Ok(());
        }
    }
}

fn create_packet(size: usize) -> String {
    let data: String = "Some data!".into();
    let loops = size / data.len();
    let mut buffer = String::with_capacity(size);
    for _ in 0..loops {
        buffer.push_str("Some data!");
    }
    buffer
}
