use std::process;

use mqtt_channel::Message;
use mqtt_channel::PubChannel;
use mqtt_channel::Topic;
use mqtt_channel::TopicFilter;
use serde_json::json;
use time::OffsetDateTime;

pub fn health_check_topics(daemon_name: &str) -> TopicFilter {
    vec![
        "tedge/health-check".into(),
        format!("tedge/health-check/{daemon_name}"),
    ]
    .try_into()
    .expect("Invalid topic filter")
}

pub async fn send_health_status(responses: &mut impl PubChannel, daemon_name: &str) {
    let response_topic_health =
        Topic::new_unchecked(format!("tedge/health/{daemon_name}").as_str());

    let health_status = json!({
        "status": "up",
        "pid": process::id(),
        "time": OffsetDateTime::now_utc().unix_timestamp(),
    })
    .to_string();

    let health_message = Message::new(&response_topic_health, health_status).with_retain();
    let _ = responses.send(health_message).await;
}

pub fn health_status_down_message(daemon_name: &str) -> Message {
    Message {
        topic: Topic::new_unchecked(&format!("tedge/health/{daemon_name}")),
        payload: json!({
            "status": "down",
            "pid": process::id()})
        .to_string()
        .into(),
        qos: mqtt_channel::QoS::AtLeastOnce,
        retain: true,
    }
}

pub fn health_status_up_message(daemon_name: &str) -> Message {
    let response_topic_health =
        Topic::new_unchecked(format!("tedge/health/{daemon_name}").as_str());

    let health_status = json!({
        "status": "up",
        "pid": process::id(),
        "time": OffsetDateTime::now_utc().unix_timestamp(),
    })
    .to_string();

    Message::new(&response_topic_health, health_status)
        .with_qos(mqtt_channel::QoS::AtLeastOnce)
        .with_retain()
}

pub fn is_bridge_health(topic: &str) -> bool {
    if topic.starts_with("tedge/health") {
        let substrings: Vec<String> = topic.split('/').map(String::from).collect();
        if substrings.len() > 2 {
            let bridge_splits: Vec<&str> = substrings[2].split('-').collect();
            matches!(bridge_splits[..], ["mosquitto", _, "bridge"])
        } else {
            false
        }
    } else {
        false
    }
}
