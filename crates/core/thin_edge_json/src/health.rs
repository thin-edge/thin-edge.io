use std::process;

use mqtt_channel::{Message, PubChannel, Topic};
use serde_json::json;
use time::OffsetDateTime;

pub fn health_check_topics(daemon_name: &str) -> Vec<String> {
    vec![
        "tedge/health-check".into(),
        format!("tedge/health-check/{daemon_name}"),
    ]
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
