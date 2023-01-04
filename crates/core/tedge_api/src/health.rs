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

    let health_message = Message::new(&response_topic_health, health_status);
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

pub async fn get_health_status_message(daemon_name: &str) -> Message {
    let response_topic_health =
        Topic::new_unchecked(format!("tedge/health/{daemon_name}").as_str());

    let health_status = json!({
        "status": "up",
        "pid": process::id(),
        "time": OffsetDateTime::now_utc().unix_timestamp(),
    })
    .to_string();

    Message::new(&response_topic_health, health_status)
}
