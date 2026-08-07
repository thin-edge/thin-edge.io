use crate::ConfigPublisherBuilder;
use serde_json::json;
use serde_json::Value;
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

const CONFIG_TOPIC: &str = "te/device/main/service/tedge-agent/config";

fn setup(exposed_config: Vec<(&str, Option<Value>)>) -> SimpleMessageBox<MqttMessage, MqttMessage> {
    let mqtt_schema = MqttSchema::default();
    let service_topic_id = EntityTopicId::default_main_service("tedge-agent")
        .unwrap()
        .into();
    let exposed_config = exposed_config
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
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

fn config_message(json: &str) -> MqttMessage {
    MqttMessage::new(&tedge_mqtt_ext::Topic::new_unchecked(CONFIG_TOPIC), json).with_retain()
}

#[tokio::test]
async fn publishes_the_set_keys_as_one_json_object_on_startup() {
    let mut mqtt_box = setup(vec![("device.id", Some(json!("my-device")))]);

    mqtt_box
        .assert_received([config_message(r#"{"device.id":"my-device"}"#)])
        .await;
}

/// Values are published with the type they have in `tedge.toml`, so a consumer doesn't have to
/// re-parse a number or a flag out of a string.
#[tokio::test]
async fn startup_publish_keeps_the_type_of_each_value() {
    let mut mqtt_box = setup(vec![
        ("device.id", Some(json!("my-device"))),
        ("mqtt.client.port", Some(json!(8883))),
        ("c8y.enable.log_upload", Some(json!(true))),
        ("c8y.smartrest.templates", Some(json!(["1234", "5678"]))),
    ]);

    mqtt_box
        .assert_received([config_message(
            r#"{"c8y.enable.log_upload":true,"c8y.smartrest.templates":["1234","5678"],"device.id":"my-device","mqtt.client.port":8883}"#,
        )])
        .await;
}

#[tokio::test]
async fn startup_publish_omits_unset_keys() {
    let mut mqtt_box = setup(vec![
        ("device.id", Some(json!("my-device"))),
        ("c8y.url", None),
    ]);

    mqtt_box
        .assert_received([config_message(r#"{"device.id":"my-device"}"#)])
        .await;
}

#[tokio::test]
async fn republishes_a_diverged_document() {
    let mut mqtt_box = setup(vec![("device.id", Some(json!("my-device")))]);

    // Skip the startup publish
    mqtt_box.skip(1).await;

    mqtt_box
        .send(config_message(r#"{"device.id":"tampered-value"}"#))
        .await
        .unwrap();

    mqtt_box
        .assert_received([config_message(r#"{"device.id":"my-device"}"#)])
        .await;
}

#[tokio::test]
async fn an_empty_payload_is_corrected_back_to_the_expected_document() {
    let mut mqtt_box = setup(vec![("device.id", Some(json!("my-device")))]);
    mqtt_box.skip(1).await;

    mqtt_box.send(config_message("")).await.unwrap();

    mqtt_box
        .assert_received([config_message(r#"{"device.id":"my-device"}"#)])
        .await;
}

#[tokio::test]
async fn payload_matching_the_expected_document_does_not_trigger_a_republish() {
    let mut mqtt_box = setup(vec![("device.id", Some(json!("my-device")))]);
    mqtt_box.skip(1).await;

    // The actor's own document, replayed back to it (e.g. by the broker), matches expected state
    mqtt_box
        .send(config_message(r#"{"device.id":"my-device"}"#))
        .await
        .unwrap();

    let next = tokio::time::timeout(std::time::Duration::from_millis(200), mqtt_box.recv()).await;
    assert!(next.is_err(), "no message should have been published");
}

#[tokio::test]
async fn a_stale_key_no_longer_in_the_exposed_set_is_dropped_by_the_republish() {
    let mut mqtt_box = setup(vec![("device.id", Some(json!("my-device")))]);
    mqtt_box.skip(1).await;

    // A retained document from a previous version still carries a renamed/removed/demoted key
    mqtt_box
        .send(config_message(
            r#"{"device.id":"my-device","old.key":"leftover-value"}"#,
        ))
        .await
        .unwrap();

    // The whole document is replaced as a unit, so the stale key is simply absent — there is no
    // separate clearing path to trigger.
    mqtt_box
        .assert_received([config_message(r#"{"device.id":"my-device"}"#)])
        .await;
}
