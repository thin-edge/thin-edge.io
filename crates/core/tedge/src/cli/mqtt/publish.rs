use crate::cli::mqtt::MqttError;
use crate::command::Command;
use camino::Utf8PathBuf;
use certificate::parse_root_certificate;
use rumqttc::tokio_rustls::rustls::ClientConfig;
use rumqttc::tokio_rustls::rustls::RootCertStore;
use rumqttc::Event;
use rumqttc::Incoming;
use rumqttc::MqttOptions;
use rumqttc::Outgoing;
use rumqttc::Packet;
use rumqttc::QoS::AtLeastOnce;
use rumqttc::QoS::AtMostOnce;
use rumqttc::QoS::ExactlyOnce;
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
    pub retain: bool,
    pub ca_file: Option<Utf8PathBuf>,
    pub ca_path: Option<Utf8PathBuf>,
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

    if cmd.ca_file.is_some() || cmd.ca_path.is_some() {
        let mut root_store = RootCertStore::empty();

        if let Some(ca_file) = cmd.ca_file.clone() {
            parse_root_certificate::add_certs_from_file(&mut root_store, ca_file)?;
        }

        if let Some(ca_path) = cmd.ca_path.clone() {
            parse_root_certificate::add_certs_from_directory(&mut root_store, ca_path)?;
        }

        const INSECURE_MQTT_PORT: u16 = 1883;
        const SECURE_MQTT_PORT: u16 = 8883;

        if cmd.port == INSECURE_MQTT_PORT && !root_store.is_empty() {
            eprintln!("Warning: Connecting on port 1883 for insecure MQTT using a TLS connection");
        }
        if cmd.port == SECURE_MQTT_PORT && root_store.is_empty() {
            eprintln!("Warning: Connecting on port 8883 for secure MQTT with no CA certificates");
        }

        let tls_config = ClientConfig::builder()
            .with_safe_defaults()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        options.set_transport(rumqttc::Transport::tls_with_config(tls_config.into()));
    }

    let payload = cmd.message.as_bytes();

    let (mut client, mut connection) = rumqttc::Client::new(options, DEFAULT_QUEUE_CAPACITY);
    let mut published = false;
    let mut acknowledged = false;
    let mut any_error = None;

    client.publish(&cmd.topic, cmd.qos, cmd.retain, payload)?;

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
                any_error = Some(MqttError::ServerConnection("Disconnected".to_string()));
                break;
            }
            Err(err) => {
                any_error = Some(err.into());
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