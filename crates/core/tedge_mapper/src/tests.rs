use std::time::Duration;

use mqtt_tests::with_timeout::{Maybe, WithTimeout};
use serial_test::serial;
use tokio::task::JoinHandle;

use crate::{
    c8y_converter::CumulocityConverter, mapper::create_mapper, size_threshold::SizeThreshold,
};

const TEST_TIMEOUT_MS: Duration = Duration::from_millis(5000);

#[tokio::test]
#[serial]
async fn mapper_publishes_supported_operations_smartrest_message_on_init() {
    let broker = mqtt_tests::test_mqtt_broker();

    let mut messages = broker.messages_published_on("c8y/s/us").await;

    // Start the C8Y Mapper
    let c8y_mapper = start_c8y_mapper(broker.port).await.unwrap();

    // Expect SmartREST message 114 for supported operations on c8y/s/us topic
    let msg = messages
        .recv()
        .with_timeout(TEST_TIMEOUT_MS)
        .await
        .expect_or("No message received after a second.");
    dbg!(&msg);
    assert!(&msg.contains("114"));

    c8y_mapper.abort();
}

async fn start_c8y_mapper(mqtt_port: u16) -> Result<JoinHandle<()>, anyhow::Error> {
    let device_name = "test-device".into();
    let size_threshold = SizeThreshold(16 * 1024);
    let converter = Box::new(CumulocityConverter::new(size_threshold, device_name));

    let mut mapper = create_mapper("c8y-mapper-test", mqtt_port, converter).await?;

    let mapper_task = tokio::spawn(async move {
        let _ = mapper.run().await;
    });
    Ok(mapper_task)
}
