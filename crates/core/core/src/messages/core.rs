use actix::prelude::*;
use mqtt_channel::{Message as MqttMessage, Payload};

#[derive(Clone, Debug)]
pub struct Ping(usize);

impl Message for Ping {
    type Result = ();
}

#[derive(Clone, Debug, Message)]
#[rtype(result = "()")]
pub struct MqttPayload {
    pub mqtt_msg: MqttMessage,
}

impl MqttPayload {
    pub fn new(mqtt_msg: MqttMessage) -> Self {
        Self { mqtt_msg }
    }
}

#[derive(Debug, Message)]
#[rtype(result = "()")]
pub struct RegisterSender {
    addr: Recipient<MqttPayload>,
}

impl RegisterSender {
    pub fn new(addr: Recipient<MqttPayload>) -> Self {
        Self { addr }
    }

    pub fn recipient(&self) -> &Recipient<MqttPayload> {
        &self.addr
    }
}

#[derive(Debug, Message)]
#[rtype(result = "()")]
#[allow(dead_code)]
pub struct Measurement {
    payload: Payload,
}

#[derive(Debug, Message)]
#[rtype(result = "()")]
pub struct MqttResponse {}
