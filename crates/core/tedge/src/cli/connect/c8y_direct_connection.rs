use super::{BridgeConfig, ConnectError};
use rumqttc::{
    self, certs, pkcs8_private_keys, Client, Event, Incoming, MqttOptions, Outgoing, Packet, QoS,
    Transport,
};

use rustls_0_19::ClientConfig;
use std::{fs::File, io::BufReader};
use tedge_users::UserManager;

// Connect directly to the c8y cloud over mqtt and publish device create message.
pub fn create_device_with_direct_connection(
    user_manager: UserManager,
    bridge_config: &BridgeConfig,
) -> Result<(), ConnectError> {
    const DEVICE_ALREADY_EXISTS: &[u8] = b"41,100,Device already existing";
    const DEVICE_CREATE_ERROR_TOPIC: &str = "s/e";

    let address = bridge_config.address.clone();
    let host: Vec<&str> = address.split(":").collect();

    let mut mqtt_options = MqttOptions::new(bridge_config.remote_clientid.clone(), host[0], 8883);
    mqtt_options.set_keep_alive(std::time::Duration::from_secs(5));

    let mut client_config = ClientConfig::new();
    // Use rustls-native-certs to load root certificates from the operating system.
    client_config.root_store =
        rustls_native_certs::load_native_certs().expect("Failed to load platform certificates.");

    let pvt_key = read_pvt_key(user_manager, bridge_config.bridge_keyfile.clone())?;
    let cert_chain = read_cert_chain(bridge_config.bridge_certfile.clone())?;

    let _ = client_config.set_single_client_cert(cert_chain, pvt_key);

    mqtt_options.set_transport(Transport::tls_with_config(client_config.into()));

    let (mut client, mut connection) = Client::new(mqtt_options, 10);

    client.subscribe(DEVICE_CREATE_ERROR_TOPIC, QoS::AtLeastOnce)?;

    let mut device_create_try: usize = 0;
    for event in connection.iter() {
        match event {
            Ok(Event::Incoming(Packet::SubAck(_))) => {
                publish_device_create_message(&mut client, &bridge_config.remote_clientid.clone())?;
            }
            Ok(Event::Incoming(Packet::Publish(response))) => {
                // We got a response
                if response.payload == DEVICE_ALREADY_EXISTS {
                    return Ok(());
                }
            }
            Ok(Event::Outgoing(Outgoing::PingReq)) => {
                // If not received any response then resend the device create request again.
                // else timeout.
                if device_create_try < 1 {
                    publish_device_create_message(
                        &mut client,
                        &bridge_config.remote_clientid.clone(),
                    )?;
                    device_create_try += 1;
                } else {
                    // No messages have been received for a while
                    break;
                }
            }
            Ok(Event::Incoming(Incoming::Disconnect)) => {
                eprintln!("ERROR: Disconnected");
                break;
            }
            Err(err) => {
                eprintln!("ERROR: {:?}", err);
                return Err(ConnectError::ConnectionCheckError);
            }
            _ => {}
        }
    }

    // The request has not even been sent
    println!("No response from Cumulocity");
    Err(ConnectError::TimeoutElapsedError)
}

fn publish_device_create_message(client: &mut Client, device_id: &str) -> Result<(), ConnectError> {
    const DEVICE_CREATE_PUBLISH_TOPIC: &str = "s/us";
    const DEVICE_TYPE: &str = "thin-edge.io";
    client.publish(
        DEVICE_CREATE_PUBLISH_TOPIC,
        QoS::ExactlyOnce,
        false,
        format!("100,{},{}", device_id, DEVICE_TYPE).as_bytes(),
    )?;
    Ok(())
}

fn read_pvt_key(
    user_manager: UserManager,
    key_file: tedge_config::FilePath,
) -> Result<rustls_0_19::PrivateKey, ConnectError> {
    // Become BROKER_USER to read the private key
    let _user_guard = user_manager.become_user(tedge_users::BROKER_USER)?;
    let f = File::open(key_file)?;
    let mut key_reader = BufReader::new(f);
    let result = pkcs8_private_keys(&mut key_reader);
    match result {
        Ok(key) => Ok(key[0].clone()),
        Err(_) => {
            return Err(ConnectError::RumqttcPrivateKey);
        }
    }
}

fn read_cert_chain(
    cert_file: tedge_config::FilePath,
) -> Result<Vec<rustls_0_19::Certificate>, ConnectError> {
    let f = File::open(cert_file)?;
    let mut cert_reader = BufReader::new(f);
    let result = certs(&mut cert_reader);
    let cert_chain: Vec<rustls_0_19::Certificate> = match result {
        Ok(cert) => cert,
        Err(_) => {
            return Err(ConnectError::RumqttcCertificate);
        }
    };
    Ok(cert_chain)
}
