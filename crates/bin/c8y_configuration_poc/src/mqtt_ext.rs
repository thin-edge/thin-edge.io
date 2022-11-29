use tedge_actors::{Recipient, RuntimeHandle};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MqttConfig {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MqttMessage {
    pub topic: String,
    pub payload: String,
}

/// Open a new MQTT connection using the given config
///
/// A call to `let pub_messages = new_connection(runtime, config, sub_messages)`
/// spawn an MQTT actor and returns a `pub_messages` recipient of `MqttMessage`.
/// * The `sub_messages` recipient argument is where
///   the messages received from the connection will be send to.
/// * The `pub_messages` returned recipient is where
///   the callee will have to send messages to have them published over MQTT.
pub fn new_connection(
    runtime: &mut RuntimeHandle,
    config: MqttConfig,
    sub_messages: Recipient<MqttMessage>,
) -> Recipient<MqttMessage> {
    todo!()
}
