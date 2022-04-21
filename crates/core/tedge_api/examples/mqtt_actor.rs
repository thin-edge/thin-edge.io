use tedge_api::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    Ok(())
}

/// Messages exchanged over the network
enum MqttPacket {
    Pub {
        id: u16,
        topic: String,
        payload: String,
    },
    PubAck {
        id: u16,
    },
}

/// Messages exchanged with the client
struct MqttMessage {
    topic: String,
    payload: String,
}

/// Messages exchanged with the clock
struct TimeoutRequest {
    duration: u64,
}

struct Timeout {
    timestamp: u64,
}

/// An MQTTConnection
struct MQTTConnection {}
impl Handler<MqttPacket> for MQTTConnection {}
impl Handler<MqttMessage> for MQTTConnection {}
impl Handler<Timeout> for MQTTConnection {}

struct NetworkConnection {}
impl Handler<MqttPacket> for NetworkConnection {}
impl Producer<MqttPacket> for NetworkConnection {}

struct MQTTClient {}
impl Producer<MqttMessage> for MQTTClient {}
impl Handler<MqttMessage> for MQTTClient {}

struct Clock {}
impl Handler<TimeoutRequest> for Clock {}
impl Producer<Timeout> for Clock {}
