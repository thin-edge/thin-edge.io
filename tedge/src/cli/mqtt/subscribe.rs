use crate::cli::mqtt::MqttError;
use crate::command::Command;
use rumqttc::QoS;
use rumqttc::{Client, Event, Incoming, MqttOptions, Packet};

const DEFAULT_QUEUE_CAPACITY: usize = 10;

pub struct MqttSubscribeCommand {
    pub host: String,
    pub port: u16,
    pub topic: String,
    pub qos: QoS,
    pub hide_topic: bool,
    pub client_id: String,
}

impl Command for MqttSubscribeCommand {
    fn description(&self) -> String {
        format!(
            "subscribe the topic \"{}\" with QoS \"{:?}\".",
            self.topic, self.qos
        )
    }

    fn execute(&self) -> anyhow::Result<()> {
        Ok(subscribe(self)?)
    }
}

fn subscribe(cmd: &MqttSubscribeCommand) -> Result<(), MqttError> {
    let mut options = MqttOptions::new(cmd.client_id.as_str(), &cmd.host, cmd.port);
    options.set_clean_session(true);

    let (mut client, mut connection) = Client::new(options, DEFAULT_QUEUE_CAPACITY);

    for event in connection.iter() {
        match event {
            Ok(Event::Incoming(Packet::Publish(message))) => {
                // trims the trailing null char if one exists
                let payload = message
                    .payload
                    .strip_suffix(&[0])
                    .unwrap_or(&message.payload);
                match std::str::from_utf8(payload) {
                    Ok(payload) => {
                        if cmd.hide_topic {
                            println!("{}", &payload);
                        } else {
                            println!("[{}] {}", &message.topic, payload);
                        }
                    }
                    Err(err) => {
                        eprintln!("ERROR: {}", err);
                    }
                }
            }
            Ok(Event::Incoming(Incoming::Disconnect)) => {
                eprintln!("INFO: Disconnected");
                break;
            }
            Ok(Event::Incoming(Packet::ConnAck(_))) => {
                eprintln!("INFO: Connected");
                client.subscribe(cmd.topic.as_str(), cmd.qos).unwrap();
            }
            Err(err) => {
                let err_msg = err.to_string();
                if err_msg.contains("I/O: Connection refused (os error 111)") {
                    return Err(MqttError::ServerConnection(err_msg));
                }

                eprintln!("ERROR: {}", err_msg);
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
            _ => {}
        }
    }

    Ok(())
}
