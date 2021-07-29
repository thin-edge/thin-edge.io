use futures::future::TryFutureExt;
use mqtt_client::{Client, Message, MqttClient, MqttClientError, QoS, Topic, TopicFilter};
use rumqttc::StateError;
mod rumqttd_broker;

use tokio::time::Duration;
#[derive(Debug)]
enum TestJoinError {
    TestMqttClientError(MqttClientError),
    ElapseTime,
}

#[tokio::test]
// This checks the mqtt packets are within the limit or not
async fn packet_size_within_limit() -> Result<(), anyhow::Error> {
    // Start the local broker
    let mqtt_server_handle = tokio::spawn(async {
        rumqttd_broker::start_broker_local("../../configuration/rumqttd/rumqttd_5883.conf").await
    });
    // Start the subscriber
    let subscriber = tokio::spawn(async move { subscribe_until_3_messages_received().await });

    // Start the publisher and publish 3 messages
    let publisher = tokio::spawn(async move { publish_3_messages().await });

    let _ = publisher.await?;
    let res = subscriber.await?;
    mqtt_server_handle.abort();
    match res {
        Err(e) => {
            return Err(e);
        }
        _ => {
            return Ok(());
        }
    }
}

#[tokio::test]
// This checks the mqtt packet size that exceeds the limit
async fn packet_size_exceeds_limit() -> Result<(), anyhow::Error> {
    // Start the broker
    let mqtt_server_handle = tokio::spawn(async {
        rumqttd_broker::start_broker_local("../../configuration/rumqttd/rumqttd_5884.conf").await
    });

    // Start the publisher and publish a message
    let publish = tokio::spawn(async { publish_big_message_wait_for_error().await });

    // if error is received then test is ok, else test should fail
    let res = publish.await?;
    mqtt_server_handle.abort();
    match res {
        Err(e) => {
            return Err(e);
        }
        _ => {
            return Ok(());
        }
    }
}

async fn subscribe_errors(pub_client: &Client) -> Result<(), MqttClientError> {
    let mut errors = pub_client.subscribe_errors();
    // return particular error else return Ok
    while let Some(error) = errors.next().await {
        match *error {
            MqttClientError::ConnectionError(rumqttc::ConnectionError::MqttState(
                StateError::Deserialization(rumqttc::Error::PayloadTooLong),
            )) => {
                return Err(mqtt_client::MqttClientError::ConnectionError(
                    rumqttc::ConnectionError::Mqtt4Bytes(rumqttc::Error::PayloadTooLong),
                ));
            }
            _ => {
                return Ok(());
            }
        }
    }

    Ok(())
}

async fn subscribe_until_3_messages_received() -> Result<(), anyhow::Error> {
    let sub_filter = TopicFilter::new("test/hello")?;
    let client =
        Client::connect("subscribe", &mqtt_client::Config::default().with_port(5883)).await?;
    let mut messages = client.subscribe(sub_filter).await?;
    let mut cnt: i32 = 0;
    while let Some(_message) = messages.next().await {
        if cnt >= 3 {
            break;
        } else {
            cnt += 1;
        }
    }
    assert!(cnt >= 3);
    client.disconnect().await?;
    Ok(())
}

async fn publish_3_messages() -> Result<(), anyhow::Error> {
    // create a 128MB message
    let buffer = create_packet(134217728);
    let topic = Topic::new("test/hello")?;
    let client = Client::connect(
        "publish_big_data",
        &mqtt_client::Config::default().with_port(5883),
    )
    .await?;
    let message = Message::new(&topic, buffer.clone()).qos(QoS::AtMostOnce);
    let mut cnt: i32 = 0;
    loop {
        let () = client.publish(message.clone()).await?;
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

async fn publish_big_message_wait_for_error() -> Result<(), anyhow::Error> {
    // create a 260MB message
    let buffer = create_packet(272629760);

    let topic = Topic::new("test/hello")?;
    let publish_client = Client::connect(
        "publish_big_data",
        &mqtt_client::Config::default().with_port(5884),
    )
    .await?;

    let message = Message::new(&topic, buffer.clone()).qos(QoS::ExactlyOnce);

    let publish_handle = publish_client.publish(message);

    // wait for error else timeout
    let timeout = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        subscribe_errors(&publish_client).map_err(|e| TestJoinError::TestMqttClientError(e)),
    )
    .map_err(|_e| TestJoinError::ElapseTime);

    // wait until one of the future returns error
    let res = tokio::try_join!(
        timeout,
        publish_handle.map_err(|e| TestJoinError::TestMqttClientError(e))
    );

    match res {
        Ok((first, _second)) => match first {
            Err(TestJoinError::TestMqttClientError(_)) => {
                return Ok(());
            }
            _ => {
                anyhow::bail!("Did not catch error correctly");
            }
        },
        _ => {
            anyhow::bail!("test failed");
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
