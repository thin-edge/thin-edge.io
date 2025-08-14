use crate::twin_manager::builder::TwinManagerActorBuilder;
use crate::twin_manager::builder::TwinManagerConfig;
use serde_json::json;
use serde_json::Value;
use tedge_actors::test_helpers::MessageReceiverExt;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_mqtt_ext::MqttMessage;
use tedge_test_utils::fs::TempTedgeDir;

#[tokio::test]
async fn process_inventory_json_content_on_init() {
    let inventory_json = json!({
        "boolean_key": true,
        "numeric_key": 10,
        "string_key": "value"
    });
    let handle = setup(inventory_json);
    let mut mqtt_box = handle.mqtt_box;
    mqtt_box
        .send(
            MqttMessage::from(("te/device/main/service/tedge-agent/status/health", "1"))
                .with_retain(),
        )
        .await
        .unwrap(); // Skip the above twin update

    mqtt_box
        .assert_received([
            MqttMessage::from(("te/device/main///twin/boolean_key", "true")).with_retain(),
            MqttMessage::from(("te/device/main///twin/numeric_key", "10")).with_retain(),
            MqttMessage::from(("te/device/main///twin/string_key", "\"value\"")).with_retain(),
        ])
        .await;
}

#[tokio::test]
async fn inventory_json_value_ignored_if_twin_data_present() {
    let inventory_json = json!({
        "x": 1,
        "y": 2,
        "z": 3,
    });
    let handle = setup(inventory_json);
    let mut mqtt_box = handle.mqtt_box;

    mqtt_box
        .send(MqttMessage::from(("te/device/main///twin/y", "5")).with_retain())
        .await
        .unwrap(); // Skip the above twin update

    mqtt_box
        .assert_received([
            MqttMessage::from(("te/device/main///twin/x", "1")).with_retain(),
            MqttMessage::from(("te/device/main///twin/z", "3")).with_retain(),
        ])
        .await;
}

pub(crate) struct TestHandle {
    pub _tmp_dir: TempTedgeDir,
    pub mqtt_box: SimpleMessageBox<MqttMessage, MqttMessage>,
}

pub fn setup(inventory_json: Value) -> TestHandle {
    let mqtt_schema = MqttSchema::default();
    let tmp_dir = TempTedgeDir::default();
    let config_dir = tmp_dir.utf8_path_buf();
    create_inventory_json_file_with_content(&tmp_dir, &inventory_json.to_string());

    let main_device_id = EntityTopicId::default_main_device();
    let config = TwinManagerConfig::new(
        config_dir,
        mqtt_schema.clone(),
        main_device_id.clone(),
        main_device_id
            .default_service_for_device("tedge-agent")
            .unwrap(),
    );

    let mut mqtt_actor = SimpleMessageBoxBuilder::new("MQTT", 64);
    let actor = TwinManagerActorBuilder::new(config, &mut mqtt_actor).build();
    let mqtt_box = mqtt_actor.build();

    tokio::spawn(async move { actor.run().await });

    TestHandle {
        _tmp_dir: tmp_dir,
        mqtt_box,
    }
}

fn create_inventory_json_file_with_content(ttd: &TempTedgeDir, content: &str) {
    let file = ttd.dir("device").file("inventory.json");
    file.with_raw_content(content);
}
