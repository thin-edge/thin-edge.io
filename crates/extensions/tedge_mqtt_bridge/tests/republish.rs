use anyhow::anyhow;
use anyhow::Context;
use mqttbytes::QoS;
use rumqttc::MqttOptions;
use std::str::from_utf8;
use std::sync::Arc;
use tedge_config::TEdgeConfig;
use tedge_mqtt_bridge::BridgeConfig;
use tedge_mqtt_bridge::MqttBridgeActorBuilder;
use test_broker::TestMqttBroker;
use tracing::warn;

mod test_broker;

async fn new_broker() -> Arc<TestMqttBroker> {
    let broker = Arc::new(TestMqttBroker::new().await.unwrap());
    {
        let broker = broker.clone();
        tokio::spawn(async move { broker.start().await });
    }
    broker
}

async fn start_mqtt_bridge(
    local_port: u16,
    cloud_port: u16,
    rules: BridgeConfig,
    tedge_config: Option<TEdgeConfig>,
) {
    let cloud_config = MqttOptions::new("cloud-device", "127.0.0.1", cloud_port);
    let service_name = "tedge-mapper-test";
    let health_topic = format!("te/device/main/service/{service_name}/status/health")
        .as_str()
        .try_into()
        .unwrap();
    MqttBridgeActorBuilder::new(
        &tedge_config.unwrap_or_else(|| tedge_mqtt_config(local_port)),
        service_name,
        &health_topic,
        rules,
        cloud_config,
    )
    .await;
}

const HEALTH: &str = "te/device/main/#";

#[tokio::test]
async fn bridge_republishes_messages_to_cloud_on_error() {
    // rumqttc > 0.24 fixes the handling of clean sessions and stopped
    // republishing unacknowledged messages when we reconnect. This test checks
    // that we handle the unacknowledged messages correctly.
    std::env::set_var("RUST_LOG", "rumqttd=debug,tedge_mqtt_bridge=debug,info");
    let _ = env_logger::try_init();
    let local_broker = new_broker().await;
    let cloud_broker = new_broker().await;

    let mut rules = BridgeConfig::new();
    rules.forward_from_local("s/us", "c8y/", "").unwrap();
    rules.forward_from_remote("s/ds", "c8y/", "").unwrap();

    start_mqtt_bridge(local_broker.port(), cloud_broker.port(), rules, None).await;

    wait_until_health_status_is("up", &local_broker)
        .await
        .unwrap();

    cloud_broker.disable_acknowledgements().await;

    local_broker
        .publish_to_clients("c8y/s/us", b"a,fake,smartrest,message", QoS::AtLeastOnce)
        .await
        .unwrap();

    cloud_broker.next_message_matching("s/us").await;

    cloud_broker.enable_acknowledgements().await;
    cloud_broker.disconnect_clients_abruptly().await;

    // Check the message actually gets delivered
    cloud_broker.next_message_matching("s/us").await;

    // And then that the acknowledgement is successfully forwarded
    local_broker.wait_until_all_messages_acked().await;

    let publishes = local_broker.sent_publishes().await;
    let acks = local_broker.received_acks().await;
    let publish_pkids = publishes
        .iter()
        .map(|publish| publish.pkid)
        .collect::<Vec<_>>();
    let ack_pkids = acks.iter().map(|ack| ack.pkid).collect::<Vec<_>>();

    // Verify all the messages were acknowledged in order
    assert_eq!(publish_pkids, ack_pkids);
}

#[tokio::test]
async fn bridge_republishes_messages_to_local_on_error() {
    // The local connection differs from the cloud connection in that it uses a
    // non-clean session, so rumqttc will republish for us. We should check this
    // happens
    std::env::set_var("RUST_LOG", "rumqttd=debug,tedge_mqtt_bridge=debug,info");
    let _ = env_logger::try_init();
    let local_broker = new_broker().await;
    let cloud_broker = new_broker().await;

    let mut rules = BridgeConfig::new();
    rules.forward_from_local("s/us", "c8y/", "").unwrap();
    rules.forward_from_remote("s/ds", "c8y/", "").unwrap();

    start_mqtt_bridge(local_broker.port(), cloud_broker.port(), rules, None).await;

    wait_until_health_status_is("up", &local_broker)
        .await
        .unwrap();

    local_broker.disable_acknowledgements().await;

    cloud_broker
        .publish_to_clients("s/ds", b"a,fake,smartrest,message", QoS::AtLeastOnce)
        .await
        .unwrap();

    local_broker.next_message_matching("c8y/s/ds").await;

    local_broker.enable_acknowledgements().await;
    local_broker.disconnect_clients_abruptly().await;

    // Check the message actually gets delivered
    local_broker.next_message_matching("c8y/s/ds").await;

    // And then that the acknowledgement is successfully forwarded
    cloud_broker.wait_until_all_messages_acked().await;

    let publishes = cloud_broker.sent_publishes().await;
    let acks = cloud_broker.received_acks().await;
    let publish_pkids = publishes
        .iter()
        .map(|publish| publish.pkid)
        .collect::<Vec<_>>();
    let ack_pkids = acks.iter().map(|ack| ack.pkid).collect::<Vec<_>>();

    // Verify all the messages were acknowledged in order
    assert_eq!(publish_pkids, ack_pkids);
}

#[tokio::test]
async fn bridge_delivers_republishes_to_cloud_before_novel_publishes() {
    std::env::set_var("RUST_LOG", "rumqttd=debug,tedge_mqtt_bridge=debug,info");
    let _ = env_logger::try_init();
    let local_broker = new_broker().await;
    let cloud_broker = new_broker().await;

    let mut rules = BridgeConfig::new();
    rules.forward_from_local("s/us", "c8y/", "").unwrap();
    rules.forward_from_remote("s/ds", "c8y/", "").unwrap();

    let tedge_config = TEdgeConfig::load_toml_str(&format!(
        "
    mqtt.client.port = {}
    mqtt.bridge.reconnect_policy.initial_interval = \"50ms\"
    ",
        local_broker.port()
    ));
    start_mqtt_bridge(
        local_broker.port(),
        cloud_broker.port(),
        rules,
        Some(tedge_config),
    )
    .await;

    wait_until_health_status_is("up", &local_broker)
        .await
        .unwrap();

    cloud_broker.disable_acknowledgements().await;

    local_broker
        .publish_to_clients("c8y/s/us", b"a,fake,smartrest,message", QoS::AtLeastOnce)
        .await
        .unwrap();

    cloud_broker.next_message_matching("s/us").await;

    cloud_broker.enable_acknowledgements().await;
    cloud_broker.disconnect_clients_abruptly().await;
    local_broker
        .publish_to_clients("c8y/s/us", b"a,different,message", QoS::AtLeastOnce)
        .await
        .unwrap();

    local_broker.wait_until_all_messages_acked().await;

    let publishes = local_broker.sent_publishes().await;
    let acks = local_broker.received_acks().await;
    let publish_pkids = publishes
        .iter()
        .map(|publish| publish.pkid)
        .collect::<Vec<_>>();
    let ack_pkids = acks.iter().map(|ack| ack.pkid).collect::<Vec<_>>();

    // Verify all the messages were acknowledged in order
    assert_eq!(
        publish_pkids, ack_pkids,
        "MQTT Acknowledgements were not in the order the messages were published in"
    );

    let payload = String::from_utf8(
        cloud_broker
            .next_message_matching("s/us")
            .await
            .payload
            .to_vec(),
    )
    .unwrap();
    assert_eq!(payload, "a,fake,smartrest,message");
    let payload = String::from_utf8(
        cloud_broker
            .next_message_matching("s/us")
            .await
            .payload
            .to_vec(),
    )
    .unwrap();
    assert_eq!(payload, "a,different,message");
}

fn tedge_mqtt_config(mqtt_port: u16) -> TEdgeConfig {
    TEdgeConfig::load_toml_str(&format!(
        "
    mqtt.client.port = {mqtt_port}
    mqtt.bridge.reconnect_policy.initial_interval = \"0s\"
    "
    ))
}

async fn wait_until_health_status_is(status: &str, broker: &TestMqttBroker) -> anyhow::Result<()> {
    loop {
        let health = broker.next_message_matching(HEALTH).await;
        if !(health.topic.starts_with("te/device/main/service")
            && health.topic.ends_with("status/health"))
        {
            warn!(
                "Unexpected message on topic {} when looking for health status messages",
                health.topic
            );
            continue;
        }
        let payload = from_utf8(&health.payload).context("decoding health payload")?;
        let json: serde_json::Value = serde_json::from_str(payload)?;
        match (status, json["status"].as_str()) {
            ("up", Some("up")) | ("down", Some("down")) => break Ok(()),
            (_, Some("up" | "down")) => continue,
            (_, Some(status)) => {
                break Err(anyhow!(
                    "Unknown health status {status:?} in tedge-json: {payload}"
                ))
            }
            (_, None) => break Err(anyhow!("Health status missing from payload: {payload}")),
        }
    }
}
