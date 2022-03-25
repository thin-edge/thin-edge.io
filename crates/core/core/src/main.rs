use actix::prelude::*;
use anyhow::Result;
use mqtt_channel::{Config, Connection, Message as MqttMessage, UnboundedReceiver};

#[derive(Clone, Debug)]
struct Ping(usize);

impl Message for Ping {
    type Result = ();
}

#[derive(Debug, Message)]
#[rtype(result = "()")]
struct MqttPayload {}

/// Actor
#[derive(Debug)]
struct AgentCoreActor {
    requests: UnboundedReceiver<MqttMessage>,
}

impl AgentCoreActor {
    pub fn new(requests: UnboundedReceiver<MqttMessage>) -> Self {
        AgentCoreActor { requests }
    }
}

/// Declare actor and its context
impl Actor for AgentCoreActor {
    type Context = Context<Self>;
}

/// Handler for `Ping` message
impl Handler<MqttPayload> for AgentCoreActor {
    type Result = ();

    fn handle(&mut self, msg: MqttPayload, _: &mut Context<Self>) -> Self::Result {
        println!("{msg:?}");
    }
}

#[actix::main]
async fn main() -> Result<(), anyhow::Error> {
    let mqtt_config = Config::new("localhost", 1883);
    let mut mqtt = Connection::new(&mqtt_config).await?;

    let addr = AgentCoreActor {
        requests: mqtt.received,
    }
    .start();

    Ok(())
}
