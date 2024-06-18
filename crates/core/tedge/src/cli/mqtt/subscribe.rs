use crate::cli::mqtt::MqttError;
use crate::command::Command;
use camino::Utf8PathBuf;
use certificate::parse_root_certificate;
use rumqttc::tokio_rustls::rustls::ClientConfig;
use rumqttc::tokio_rustls::rustls::RootCertStore;
use rumqttc::Client;
use rumqttc::Event;
use rumqttc::Incoming;
use rumqttc::MqttOptions;
use rumqttc::Outgoing;
use rumqttc::Packet;
use rumqttc::QoS;
use std::sync::atomic::Ordering;
use tedge_config::MqttAuthClientConfig;

const DEFAULT_QUEUE_CAPACITY: usize = 10;
use super::MAX_PACKET_SIZE;

pub struct MqttSubscribeCommand {
    pub host: String,
    pub port: u16,
    pub topic: String,
    pub qos: QoS,
    pub hide_topic: bool,
    pub client_id: String,
    pub ca_file: Option<Utf8PathBuf>,
    pub ca_dir: Option<Utf8PathBuf>,
    pub client_auth_config: Option<MqttAuthClientConfig>,
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
    options.set_max_packet_size(MAX_PACKET_SIZE, MAX_PACKET_SIZE);

    if cmd.ca_file.is_some() || cmd.ca_dir.is_some() {
        let mut root_store = RootCertStore::empty();

        if let Some(ca_file) = &cmd.ca_file {
            parse_root_certificate::add_certs_from_file(&mut root_store, ca_file)?;
        }

        if let Some(ca_dir) = &cmd.ca_dir {
            parse_root_certificate::add_certs_from_directory(&mut root_store, ca_dir)?;
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
            .with_root_certificates(root_store);

        let tls_config = if let Some(client_auth) = cmd.client_auth_config.as_ref() {
            let client_cert = parse_root_certificate::read_cert_chain(&client_auth.cert_file)?;
            let client_key = parse_root_certificate::read_pvt_key(&client_auth.key_file)?;
            tls_config.with_client_auth_cert(client_cert, client_key)?
        } else {
            tls_config.with_no_client_auth()
        };

        options.set_transport(rumqttc::Transport::tls_with_config(tls_config.into()));
    }

    let (mut client, mut connection) = Client::new(options, DEFAULT_QUEUE_CAPACITY);
    let interrupted = super::disconnect_if_interrupted(client.clone());

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
            Ok(Event::Outgoing(Outgoing::Disconnect)) => {
                break;
            }
            Ok(Event::Incoming(Packet::ConnAck(_))) => {
                eprintln!("INFO: Connected");
                client.subscribe(cmd.topic.as_str(), cmd.qos).unwrap();
            }
            Err(err) => {
                if interrupted.load(Ordering::Relaxed) {
                    break;
                }
                let err_msg = err.to_string();
                eprintln!("ERROR: {}", err_msg);
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
            _ => {}
        }
    }

    Ok(())
}
