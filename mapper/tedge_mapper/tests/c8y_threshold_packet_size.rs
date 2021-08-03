use futures::future::TryFutureExt;
use librumqttd::{async_locallink, Config};
use mqtt_client::{Client, Message, MqttClient, QoS, Topic, TopicFilter};
use std::process::Command;

#[derive(Debug)]
enum TestJoinError {
    TestThresholdError,
    ElapseTime,
}

#[tokio::test]
// This checks the mqtt packet size that exceeds the limit
async fn c8y_threshold_packet_size() -> Result<(), anyhow::Error> {
    // Start the broker
    let mqtt_server_handle = tokio::spawn(async {
        start_broker_local("../../configuration/rumqttd/rumqttd_5885.conf").await
    });

    // set the port
    setup();

    // Start the publisher and publish a message
    let publish = tokio::spawn(async { publish_big_message_wait_for_error().await });

    // if error is received then test is ok, else test should fail
    let res = publish.await?;
    mqtt_server_handle.abort();
    match res {
        Err(e) => {
            cleanup();
            return Err(e);
        }
        _ => {
            cleanup();
            return Ok(());
        }
    }
}

async fn subscribe_errors(pub_client: &Client) -> Result<(), anyhow::Error> {
    let error_filter = TopicFilter::new("tedge/errors")?;
    let mut error_messages = pub_client.subscribe(error_filter).await?;
    while let Some(error) = error_messages.next().await {
        let payload_str: String = error.payload_str().unwrap().into();
        if payload_str.contains("The input size 20480 is too big. The threshold is 16384") {
            anyhow::bail!("The input size 20480 is too big. The threshold is 16384");
        } else {
            return Ok(());
        }
    }
    Ok(())
}

async fn start_broker_local(cfile: &str) -> anyhow::Result<()> {
    let config: Config = confy::load_path(cfile)?;
    let (mut router, _console, servers, _builder) = async_locallink::construct_broker(config);
    let router = tokio::task::spawn_blocking(move || -> anyhow::Result<()> { Ok(router.start()?) });
    servers.await;
    let _ = router.await;
    Ok(())
}

async fn publish_big_message_wait_for_error() -> Result<(), anyhow::Error> {
    // create a 20KB message
    let buffer = create_packet(1024 * 20);

    let topic = Topic::new("tedge/measurements")?;

    let publish_client = Client::connect(
        "publish_big_data",
        &mqtt_client::Config::default().with_port(5885),
    )
    .await?;

    let message = Message::new(&topic, buffer.clone()).qos(QoS::AtMostOnce);

    let publish_handle = publish_client.publish(message);

    // wait for error else timeout
    let timeout = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        subscribe_errors(&publish_client).map_err(|_s| TestJoinError::TestThresholdError),
    )
    .map_err(|_e| TestJoinError::ElapseTime);

    // wait until one of the future returns error
    let res = tokio::try_join!(
        timeout,
        publish_handle.map_err(|_e| TestJoinError::TestThresholdError)
    );

    match res {
        Ok((first, _second)) => match first {
            Err(TestJoinError::TestThresholdError) => {
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

fn setup() {
    // set the port
    let _tedge_output = Command::new("sudo")
        .args(&["tedge", "config", "set", "mqtt.port", "5885"])
        .output()
        .expect("failed to execute process");

    // start the tedge mapper
    let _sysctl_output = Command::new("sudo")
        .args(&["systemctl", "restart", "tedge-mapper-c8y.service"])
        .output()
        .expect("failed to execute process");
}

fn cleanup() {
    // set the port
    let _tedge_output = Command::new("sudo")
        .args(&["tedge", "config", "unset", "mqtt.port"])
        .output()
        .expect("failed to execute process");

    // start the tedge mapper
    let _sysctl_output = Command::new("sudo")
        .args(&["systemctl", "restart", "tedge-mapper-c8y.service"])
        .output()
        .expect("failed to execute process");
}
