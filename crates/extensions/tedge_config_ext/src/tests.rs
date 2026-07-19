use crate::ConfigPublisherBuilder;
use tedge_actors::test_helpers::MessageReceiverExt;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::MessageReceiver;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_mqtt_ext::MqttMessage;

fn setup(exposed_config: Vec<(&str, Option<&str>)>) -> SimpleMessageBox<MqttMessage, MqttMessage> {
    let mqtt_schema = MqttSchema::default();
    let service_topic_id = EntityTopicId::default_main_service("tedge-agent")
        .unwrap()
        .into();
    let exposed_config = exposed_config
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.map(str::to_string)))
        .collect();

    let mut mqtt_actor = SimpleMessageBoxBuilder::new("MQTT", 64);
    let actor = ConfigPublisherBuilder::new(
        mqtt_schema,
        service_topic_id,
        exposed_config,
        &mut mqtt_actor,
    )
    .build();
    let mqtt_box = mqtt_actor.build();

    tokio::spawn(async move { actor.run().await });

    mqtt_box
}

#[tokio::test]
async fn publishes_set_values_on_startup() {
    let mut mqtt_box = setup(vec![("device.id", Some("my-device"))]);

    mqtt_box
        .assert_received([MqttMessage::new(
            &tedge_mqtt_ext::Topic::new_unchecked(
                "te/device/main/service/tedge-agent/config/device.id",
            ),
            "my-device",
        )
        .with_retain()])
        .await;
}

#[tokio::test]
async fn publishes_empty_payload_for_unset_key_on_startup() {
    let mut mqtt_box = setup(vec![("c8y.url", None)]);

    mqtt_box
        .assert_received([MqttMessage::new(
            &tedge_mqtt_ext::Topic::new_unchecked(
                "te/device/main/service/tedge-agent/config/c8y.url",
            ),
            "",
        )
        .with_retain()])
        .await;
}

#[tokio::test]
async fn republishes_a_diverged_owned_value() {
    let mut mqtt_box = setup(vec![("device.id", Some("my-device"))]);

    // Skip the startup publish
    mqtt_box.skip(1).await;

    mqtt_box
        .send(
            MqttMessage::new(
                &tedge_mqtt_ext::Topic::new_unchecked(
                    "te/device/main/service/tedge-agent/config/device.id",
                ),
                "tampered-value",
            )
            .with_retain(),
        )
        .await
        .unwrap();

    mqtt_box
        .assert_received([MqttMessage::new(
            &tedge_mqtt_ext::Topic::new_unchecked(
                "te/device/main/service/tedge-agent/config/device.id",
            ),
            "my-device",
        )
        .with_retain()])
        .await;
}

#[tokio::test]
async fn republishes_a_diverged_owned_value_on_clear() {
    let mut mqtt_box = setup(vec![("device.id", Some("my-device"))]);
    mqtt_box.skip(1).await;

    // An empty payload must not be able to wipe an owned value
    mqtt_box
        .send(
            MqttMessage::new(
                &tedge_mqtt_ext::Topic::new_unchecked(
                    "te/device/main/service/tedge-agent/config/device.id",
                ),
                "",
            )
            .with_retain(),
        )
        .await
        .unwrap();

    mqtt_box
        .assert_received([MqttMessage::new(
            &tedge_mqtt_ext::Topic::new_unchecked(
                "te/device/main/service/tedge-agent/config/device.id",
            ),
            "my-device",
        )
        .with_retain()])
        .await;
}

#[tokio::test]
async fn payload_matching_current_value_does_not_trigger_a_republish() {
    let mut mqtt_box = setup(vec![("device.id", Some("my-device"))]);
    mqtt_box.skip(1).await;

    // The actor's own value, replayed back to it (e.g. by the broker), matches expected state
    mqtt_box
        .send(
            MqttMessage::new(
                &tedge_mqtt_ext::Topic::new_unchecked(
                    "te/device/main/service/tedge-agent/config/device.id",
                ),
                "my-device",
            )
            .with_retain(),
        )
        .await
        .unwrap();

    let next = tokio::time::timeout(std::time::Duration::from_millis(200), mqtt_box.recv()).await;
    assert!(next.is_err(), "no message should have been published");
}

#[tokio::test]
async fn stale_key_no_longer_in_the_exposed_set_is_cleared() {
    let mut mqtt_box = setup(vec![("device.id", Some("my-device"))]);
    mqtt_box.skip(1).await;

    // A retained message for a renamed/removed/demoted key is replayed on subscribe
    mqtt_box
        .send(
            MqttMessage::new(
                &tedge_mqtt_ext::Topic::new_unchecked(
                    "te/device/main/service/tedge-agent/config/old.key",
                ),
                "leftover-value",
            )
            .with_retain(),
        )
        .await
        .unwrap();

    mqtt_box
        .assert_received([MqttMessage::new(
            &tedge_mqtt_ext::Topic::new_unchecked(
                "te/device/main/service/tedge-agent/config/old.key",
            ),
            "",
        )
        .with_retain()])
        .await;
}

#[tokio::test]
async fn empty_payload_on_an_absent_key_does_not_trigger_a_clear() {
    let mut mqtt_box = setup(vec![("device.id", Some("my-device"))]);
    mqtt_box.skip(1).await;

    mqtt_box
        .send(
            MqttMessage::new(
                &tedge_mqtt_ext::Topic::new_unchecked(
                    "te/device/main/service/tedge-agent/config/never.exposed",
                ),
                "",
            )
            .with_retain(),
        )
        .await
        .unwrap();

    let next = tokio::time::timeout(std::time::Duration::from_millis(200), mqtt_box.recv()).await;
    assert!(
        next.is_err(),
        "an empty payload on an already-absent key must not echo into another clear"
    );
}
