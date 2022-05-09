use crate::messages::core::{MqttPayload, RegisterSender};
use actix::prelude::*;

use tracing::{debug, info};

use super::c8y::CumulocityActor;

/// CoreProducer takes all input messages and determines which other actor should be used to process them.
///
/// It is statically defined atm and supports cumulocity parser only.
#[derive(Debug)]
pub struct AgentCoreProducerActor {
    sender: Option<Recipient<MqttPayload>>,
    processor: Addr<CumulocityActor>,
}

impl AgentCoreProducerActor {
    pub fn new(processor: Addr<CumulocityActor>) -> Self {
        Self {
            processor,
            sender: None,
        }
    }
}

impl Actor for AgentCoreProducerActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        info!("Core Started");
    }
}

impl Handler<MqttPayload> for AgentCoreProducerActor {
    type Result = ();

    fn handle(&mut self, msg: MqttPayload, _ctx: &mut Context<Self>) -> Self::Result {
        debug!("AgentCoreProducerActor: Dispatching: {msg:?}");
        match &msg.mqtt_msg.topic {
            topic if topic.name.starts_with("tedge/measurements") => {
                self.processor.do_send(msg.clone())
            }
            _ => {}
        }
        let rec = self.sender.clone().unwrap();
        rec.do_send(msg);
    }
}

impl Handler<RegisterSender> for AgentCoreProducerActor {
    type Result = ();

    fn handle(&mut self, msg: RegisterSender, _ctx: &mut Self::Context) -> Self::Result {
        debug!("AgentCoreProducerActor: Registering: {msg:?}");
        self.sender = Some(msg.recipient().clone());
    }
}

/// An output MQTT messages, used when any other actor has a message to dispatch,
/// atm only logging received msgs.
#[derive(Debug)]
pub struct MqttSenderActor {}

impl Actor for MqttSenderActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        info!("MqttSenderActor: Started");
    }
}

impl Handler<MqttPayload> for MqttSenderActor {
    type Result = ();

    fn handle(&mut self, msg: MqttPayload, _ctx: &mut Self::Context) -> Self::Result {
        debug!("MqttSenderActor: Received: {msg:?}")
    }
}
