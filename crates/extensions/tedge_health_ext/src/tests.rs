use std::time::Duration;

use crate::HealthMonitorBuilder;
use crate::HealthMonitorMessageBox;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::ReceiveMessages;
use tedge_actors::ServiceConsumer;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;
use tokio::time::timeout;
const TEST_TIMEOUT: Duration = Duration::from_secs(10);

#[tokio::test]
async fn send_health_check_message_to_generic_topic() -> Result<(), anyhow::Error> {
    let mut mqtt_message_box = spawn_a_health_check_actor("health-check-service-1").await?;
    let health_check_request = MqttMessage::new(&Topic::new_unchecked("tedge/health-check"), "");
    mqtt_message_box.send(health_check_request).await.unwrap();

    if let Some(message) = timeout(TEST_TIMEOUT, mqtt_message_box.recv()).await? {
        assert!(message.payload_str()?.contains("up"))
    }

    Ok(())
}

#[tokio::test]
async fn send_health_check_message_to_service_specific_topic() -> Result<(), anyhow::Error> {
    let mut mqtt_message_box = spawn_a_health_check_actor("health-check-service-2").await?;
    let health_check_request =
        MqttMessage::new(&Topic::new_unchecked("tedge/health-check/health-test"), "");
    mqtt_message_box.send(health_check_request).await.unwrap();

    if let Some(message) = timeout(TEST_TIMEOUT, mqtt_message_box.recv()).await? {
        assert!(message.payload_str()?.contains("up"))
    }

    Ok(())
}

async fn spawn_a_health_check_actor(
    service_to_be_monitored: &str,
) -> Result<HealthMonitorMessageBox, anyhow::Error> {
    let mut health_mqtt_builder: SimpleMessageBoxBuilder<MqttMessage, MqttMessage> =
        SimpleMessageBoxBuilder::new("MQTT", 5);

    let health_actor = HealthMonitorBuilder::new(service_to_be_monitored);

    let health_actor = health_actor.with_connection(&mut health_mqtt_builder);
    let (actor, message_box) = health_actor.build();
    tokio::spawn(actor.run(message_box));

    Ok(health_mqtt_builder.build())
}
