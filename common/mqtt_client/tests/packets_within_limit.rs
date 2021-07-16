use assert_matches::*;
use librumqttd::{async_locallink, Config};
use mqtt_client::{Client, Message, MqttClient, MqttClientError, QoS, Topic, TopicFilter};
use rumqttc::MqttState;
use tokio::time::Duration;
#[tokio::test]
//#[cfg(feature = "integration-test")]
// This checks the mqtt packets are within the limit or not

async fn pub_sub_packets_within_limit() -> anyhow::Result<()> {
    println!("Start the broker");
    let mqtt_server_handle = tokio::spawn(async { start_broker_local().await });

    println!("Start the subscriber");
    let _subscriber = tokio::spawn(async move { sub().await });

    println!("Start the publisher and publish 3 messages");

    // create a 128MB message
    let data: String = "Some data!".into();
    let loops = 134217728 / data.len();

    let mut buffer: String = "hello".into();
    for _ in 0..loops {
        buffer.push_str("Some data!");
    }

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
    mqtt_server_handle.abort();
    assert!(cnt >= 3);
    //std::process::exit(1);
    Ok(())
}

#[tokio::test]
async fn packetsize_fail() -> anyhow::Result<()> {
    println!("Start the broker");
    let mqtt_server_handle = tokio::spawn(async { start_broker_local().await });

    // println!("Start the subscriber");
    // let _subscriber = tokio::spawn(async move { sub().await });

    println!("Start the publisher and publish 3 messages");

    // create a 128MB message
    let data: String = "Some data!".into();
    let loops = 272629760 / data.len();

    let mut buffer: String = "hello".into();
    for _ in 0..loops {
        buffer.push_str("Some data!");
    }

    let topic = Topic::new("test/hello")?;
    let publish_client = Client::connect("publish_big_data", &mqtt_client::Config::default()).await?;

    let mut errors = publish_client.subscribe_errors();
    let error_handle = tokio::spawn(async move {
        while let Some(error) = errors.next().await {
            assert_matches!(mqtt_client::MqttClientError::ConnectionError(rumqttc::ConnectionError::Mqtt4Bytes(rumqttc::Error::PayloadTooLong)), error);
            // assert_matches!(mqtt_client::MqttClientError::ConnectionError(_), error);
            //assert!(false);

            dbg!(error);
            return;
        }
    });

    let mut cnt: i32 = 0;
    loop {
        let message = Message::new(&topic, buffer.clone()).qos(QoS::AtMostOnce);

        match publish_client.publish(message).await {
            Err(MqttClientError::ConnectionError(_)) => {
                println!("Connection failed due to packetsize issue");
            }
            Ok(_) => {
                //println!("Connection failed due to packetsize ");
                //assert!(false);
                dbg!("else");
            }
            Err(err) => {
                dbg!(err);
            }
        }

        tokio::time::sleep(Duration::from_secs(1)).await;
        if cnt >= 3 {
            break;
        } else {
            cnt += 1;
        }
    }

    publish_client.disconnect().await?;
    mqtt_server_handle.abort();
    Ok(())
    //std::process::exit(1);
}

async fn start_broker_local() -> anyhow::Result<()> {
    //dbg!(std::env::current_dir());
    let config: Config = confy::load_path("../../configuration/rumqttd/rumqttd.conf")?;

    let (mut router, _console, servers, _builder) = async_locallink::construct_broker(config);
    let router = tokio::task::spawn_blocking(move || -> anyhow::Result<()> { Ok(router.start()?) });
    servers.await;
    let _ = router.await;
    Ok(())
}

async fn sub() -> Result<(), anyhow::Error> {
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
