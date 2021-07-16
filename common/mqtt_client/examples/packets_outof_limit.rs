use librumqttd::{async_locallink, Config};
use mqtt_client::{Client, Message, MqttClient, MqttClientError, QoS, Topic, TopicFilter};
use tokio::time::Duration;
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
    let client = Client::connect("publish_big_data", &mqtt_client::Config::default()).await?;

    let mut cnt: i32 = 0;
    loop {
        let message = Message::new(&topic, buffer.clone()).qos(QoS::AtMostOnce);

        if let Err(MqttClientError::ConnectionError::MqttState) = client.publish(message).await? {
            println!("Connection failed due to packetsize issue");
        }

        tokio::time::sleep(Duration::from_secs(1)).await;
        if cnt >= 3 {
            break;
        } else {
            cnt += 1;
        }
    }

    client.disconnect().await?;
    mqtt_server_handle.abort();
    std::process::exit(1);
}

async fn start_broker_local() -> anyhow::Result<()> {
    let config: Config = confy::load_path("configuration/rumqttd/rumqttd.conf")?;
    let (mut router, _console, servers, _builder) = async_locallink::construct_broker(config);
    let router = tokio::task::spawn_blocking(move || -> anyhow::Result<()> { Ok(router.start()?) });
    servers.await;
    let _ = router.await;
    Ok(())
}

// async fn sub() -> Result<(), anyhow::Error> {
//     let sub_filter = TopicFilter::new("test/hello")?;
//     let client = Client::connect("subscribe", &mqtt_client::Config::default()).await?;

//     let mut messages = client.subscribe(sub_filter).await?;
//     let mut cnt: i32 = 0;
//     while let Some(_message) = messages.next().await {
//         if cnt >= 3 {
//             break;
//         } else {
//             cnt += 1;
//         }
//     }
//     println!("Subscriber: Received all messages, test passed");
//     assert!(cnt >= 3);
//     client.disconnect().await?;

//     Ok(())
// }
