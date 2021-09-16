use crate::cli::mqtt::MqttError;
use crate::command::Command;
use rumqttc::QoS::{AtLeastOnce, AtMostOnce, ExactlyOnce};
use rumqttc::{Event, Incoming, MqttOptions, Outgoing, Packet};
use std::time::Duration;

const DEFAULT_QUEUE_CAPACITY: usize = 10;

pub struct MqttPublishCommand {
    pub host: String,
    pub port: u16,
    pub topic: String,
    pub message: String,
    pub qos: rumqttc::QoS,
    pub client_id: String,
    pub disconnect_timeout: Duration,
}

impl Command for MqttPublishCommand {
    fn description(&self) -> String {
        format!(
            "publish the message \"{}\" on the topic \"{}\" with QoS \"{:?}\".",
            self.message, self.topic, self.qos
        )
    }

    fn execute(&self) -> anyhow::Result<()> {
        Ok(publish(self)?)
    }
}

fn publish(cmd: &MqttPublishCommand) -> Result<(), MqttError> {
    let mut options = MqttOptions::new(cmd.client_id.as_str(), &cmd.host, cmd.port);
    options.set_clean_session(true);
    let retain_flag = false;
    let payload = cmd.message.as_bytes();

    let (mut client, mut connection) = rumqttc::Client::new(options, DEFAULT_QUEUE_CAPACITY);
    let mut published = false;
    let mut acknowledged = false;
    let mut any_error = None;

    client.publish(&cmd.topic, cmd.qos, retain_flag, payload)?;

    for event in connection.iter() {
        match event {
            Ok(Event::Outgoing(Outgoing::Publish(_))) => {
                published = true;
                if cmd.qos == AtMostOnce {
                    acknowledged = true;
                    break;
                }
            }
            Ok(Event::Incoming(Packet::PubAck(_))) => {
                if cmd.qos == AtLeastOnce {
                    acknowledged = true;
                    break;
                }
            }
            Ok(Event::Incoming(Packet::PubComp(_))) => {
                if cmd.qos == ExactlyOnce {
                    acknowledged = true;
                    break;
                }
            }
            Ok(Event::Incoming(Incoming::Disconnect)) => {
                any_error = Some(MqttError::ServerError("Disconnected".to_string()));
                break;
            }
            Err(err) => {
                any_error = Some(MqttError::ServerError(err.to_string()));
                break;
            }
            _ => {}
        }
    }

    if !published {
        eprintln!("ERROR: the message has not been published");
    } else if !acknowledged {
        eprintln!("ERROR: the message has not been acknowledged");
    }

    client.disconnect()?;
    if let Some(err) = any_error {
        Err(err)
    } else {
        Ok(())
    }
}
