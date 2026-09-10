//! Tests covering what happens when the cloud broker stops acknowledging messages
//!
//! The bridge uses manual acknowledgements, so a message is only acknowledged to the local
//! broker once the cloud broker has acknowledged the forwarded copy. If those cloud
//! acknowledgements stop arriving, the local broker's inflight window fills up and it stops
//! delivering QoS 1 messages to the bridge, which is how this failure shows up in production.

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

const HEALTH: &str = "te/device/main/#";

/// The bridge's client id on the local broker
const SERVICE_NAME: &str = "tedge-mapper-test";

#[tokio::test]
async fn forwards_and_acknowledges_every_message_after_the_cloud_resumes_acknowledging() {
    init_logging();
    let local_broker = new_broker().await;
    let cloud_broker = new_broker().await;
    start_bridge(&local_broker, &cloud_broker).await;

    // The cloud goes quiet: the messages still arrive, but nothing is acknowledged
    cloud_broker.disable_acknowledgements().await;
    for i in 0..5 {
        publish_from_local(&local_broker, i).await;
        cloud_broker.next_message_matching("s/us").await;
    }
    assert_eq!(
        cloud_broker.withheld_acknowledgement_count().await,
        5,
        "the cloud broker should be holding back an acknowledgement for every message"
    );

    // The cloud starts acknowledging again, without the connection ever dropping
    cloud_broker.enable_acknowledgements().await;
    cloud_broker.release_withheld_acknowledgements().await;

    // Forwarding must continue for messages published after the stall
    for i in 5..10 {
        publish_from_local(&local_broker, i).await;
        cloud_broker.next_message_matching("s/us").await;
    }

    local_broker.wait_until_all_messages_acked().await;
    assert_acknowledged_in_order(&local_broker).await;
}

#[tokio::test]
async fn acknowledges_messages_in_order_when_the_cloud_acknowledges_a_backlog_at_once() {
    init_logging();
    let local_broker = new_broker().await;
    let cloud_broker = new_broker().await;
    start_bridge(&local_broker, &cloud_broker).await;

    // Build up a backlog large enough that the packet ids are no longer trivially ordered
    cloud_broker.disable_acknowledgements().await;
    for i in 0..20 {
        publish_from_local(&local_broker, i).await;
        cloud_broker.next_message_matching("s/us").await;
    }

    cloud_broker.enable_acknowledgements().await;
    cloud_broker.release_withheld_acknowledgements().await;

    local_broker.wait_until_all_messages_acked().await;
    assert_acknowledged_in_order(&local_broker).await;
}

#[tokio::test]
async fn keeps_delivering_while_the_backlog_stays_within_the_inflight_window() {
    init_logging();
    let local_broker = new_broker().await;
    let cloud_broker = new_broker().await;
    local_broker.set_max_outbound_inflight(10).await;
    start_bridge(&local_broker, &cloud_broker).await;

    cloud_broker.disable_acknowledgements().await;
    for i in 0..5 {
        publish_from_local(&local_broker, i).await;
        cloud_broker.next_message_matching("s/us").await;
    }

    assert_eq!(
        local_broker.queued_publish_count(SERVICE_NAME).await,
        0,
        "the local broker should not have queued anything while inside its inflight window"
    );

    cloud_broker.enable_acknowledgements().await;
    cloud_broker.release_withheld_acknowledgements().await;

    local_broker.wait_until_all_messages_acked().await;
    assert_acknowledged_in_order(&local_broker).await;
}

#[tokio::test]
async fn resumes_delivering_after_the_inflight_window_fills_and_the_cloud_acknowledges() {
    init_logging();
    let local_broker = new_broker().await;
    let cloud_broker = new_broker().await;
    local_broker.set_max_outbound_inflight(3).await;
    start_bridge(&local_broker, &cloud_broker).await;

    cloud_broker.disable_acknowledgements().await;

    // Fill the window: these reach the bridge and are forwarded, but never acknowledged
    for i in 0..3 {
        publish_from_local(&local_broker, i).await;
        cloud_broker.next_message_matching("s/us").await;
    }

    // Past the window the local broker queues instead of delivering, so the bridge never
    // sees these and cannot forward them
    for i in 3..6 {
        publish_from_local(&local_broker, i).await;
    }
    assert_eq!(
        local_broker.queued_publish_count(SERVICE_NAME).await,
        3,
        "the local broker should have stopped delivering once its inflight window filled"
    );

    // The cloud starts acknowledging, which unblocks the whole chain
    cloud_broker.enable_acknowledgements().await;
    cloud_broker.release_withheld_acknowledgements().await;

    // Everything queued behind the window must now be forwarded
    for _ in 3..6 {
        cloud_broker.next_message_matching("s/us").await;
    }
    local_broker.wait_until_all_messages_acked().await;
    assert_eq!(
        local_broker.queued_publish_count(SERVICE_NAME).await,
        0,
        "the local broker should have drained its queue"
    );
    assert_acknowledged_in_order(&local_broker).await;
}

#[tokio::test]
async fn reconnects_and_delivers_the_message_when_the_cloud_stops_acknowledging() {
    init_logging();
    let local_broker = new_broker().await;
    let cloud_broker = new_broker().await;
    start_bridge_with_config(
        &local_broker,
        &cloud_broker,
        &format!(
            "
    mqtt.client.port = {port}
    mqtt.bridge.reconnect_policy.initial_interval = \"0s\"
    mqtt.bridge.unacked_message_timeout = \"1s\"
    ",
            port = local_broker.port()
        ),
    )
    .await;

    cloud_broker.disable_acknowledgements().await;
    publish_from_local(&local_broker, 0).await;
    cloud_broker.next_message_matching("s/us").await;

    // Acknowledgements work again, but the message the cloud is already holding will
    // never be acknowledged, so only sending it again can recover it
    cloud_broker.enable_acknowledgements().await;

    cloud_broker.next_message_matching("s/us").await;
    local_broker.wait_until_all_messages_acked().await;
}

async fn new_broker() -> Arc<TestMqttBroker> {
    let broker = Arc::new(TestMqttBroker::new().await.unwrap());
    {
        let broker = broker.clone();
        tokio::spawn(async move { broker.start().await });
    }
    broker
}

fn init_logging() {
    std::env::set_var("RUST_LOG", "tedge_mqtt_bridge=debug,info");
    let _ = env_logger::try_init();
}

/// Starts a bridge forwarding `c8y/s/us` upwards and `s/ds` downwards, and waits until it is up
async fn start_bridge(local_broker: &TestMqttBroker, cloud_broker: &TestMqttBroker) {
    let config = default_config(local_broker.port());
    start_bridge_with_config(local_broker, cloud_broker, &config).await
}

async fn start_bridge_with_config(
    local_broker: &TestMqttBroker,
    cloud_broker: &TestMqttBroker,
    config: &str,
) {
    let mut rules = BridgeConfig::new();
    rules.forward_from_local("s/us", "c8y/", "").unwrap();
    rules.forward_from_remote("s/ds", "c8y/", "").unwrap();

    let cloud_config = MqttOptions::new("cloud-device", "127.0.0.1", cloud_broker.port());
    let service_name = SERVICE_NAME;
    let health_topic = format!("te/device/main/service/{service_name}/status/health")
        .as_str()
        .try_into()
        .unwrap();
    MqttBridgeActorBuilder::new(
        &TEdgeConfig::load_toml_str(config),
        service_name,
        &health_topic,
        rules,
        cloud_config,
        None,
        // No effective limit: exercise the bridge's existing forwarding behaviour.
        268_435_455,
        <_>::default(),
    )
    .await;

    wait_until_health_status_is("up", local_broker)
        .await
        .unwrap();
}

async fn publish_from_local(broker: &TestMqttBroker, index: usize) {
    broker
        .publish_to_clients(
            "c8y/s/us",
            format!("311,message-{index}").as_bytes(),
            QoS::AtLeastOnce,
        )
        .await
        .unwrap();
}

/// Asserts every message the broker sent was acknowledged, in the order it was sent
async fn assert_acknowledged_in_order(broker: &TestMqttBroker) {
    let publish_pkids = broker
        .sent_publishes()
        .await
        .iter()
        .map(|publish| publish.pkid)
        .collect::<Vec<_>>();
    let ack_pkids = broker
        .received_acks()
        .await
        .iter()
        .map(|ack| ack.pkid)
        .collect::<Vec<_>>();

    assert_eq!(
        publish_pkids, ack_pkids,
        "the messages were not acknowledged in the order they were published"
    );
}

fn default_config(mqtt_port: u16) -> String {
    format!(
        "
    mqtt.client.port = {mqtt_port}
    mqtt.bridge.reconnect_policy.initial_interval = \"0s\"
    "
    )
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
