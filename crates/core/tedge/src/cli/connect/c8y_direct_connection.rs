use super::{BridgeConfig, ConnectError};

use rumqttc::{
    self, certs, pkcs8_private_keys, rsa_private_keys, Client, Event, Incoming, MqttOptions,
    Outgoing, Packet, QoS, Transport,
};

use rustls_0_19::ClientConfig;

use std::fs;
use std::io::{Error, ErrorKind};
use std::{fs::File, io::BufReader};
use tedge_config::FilePath;

use tedge_users::UserManager;

// Connect directly to the c8y cloud over mqtt and publish device create message.
pub fn create_device_with_direct_connection(
    user_manager: UserManager,
    bridge_config: &BridgeConfig,
    device_type: &str,
) -> Result<(), ConnectError> {
    const DEVICE_ALREADY_EXISTS: &[u8] = b"41,100,Device already existing";
    const DEVICE_CREATE_ERROR_TOPIC: &str = "s/e";

    let address = bridge_config.address.clone();
    let host: Vec<&str> = address.split(':').collect();

    let mut mqtt_options = MqttOptions::new(bridge_config.remote_clientid.clone(), host[0], 8883);
    mqtt_options.set_keep_alive(std::time::Duration::from_secs(5));

    let mut client_config = ClientConfig::new();

    let () = load_root_certs(
        &mut client_config.root_store,
        bridge_config.bridge_root_cert_path.clone(),
    )?;

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
                publish_device_create_message(
                    &mut client,
                    &bridge_config.remote_clientid.clone(),
                    device_type,
                )?;
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
                        device_type,
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

fn publish_device_create_message(
    client: &mut Client,
    device_id: &str,
    device_type: &str,
) -> Result<(), ConnectError> {
    const DEVICE_CREATE_PUBLISH_TOPIC: &str = "s/us";
    client.publish(
        DEVICE_CREATE_PUBLISH_TOPIC,
        QoS::ExactlyOnce,
        false,
        format!("100,{},{}", device_id, device_type).as_bytes(),
    )?;
    Ok(())
}

fn load_root_certs(
    root_store: &mut rustls_0_19::RootCertStore,
    cert_dir: FilePath,
) -> Result<(), ConnectError> {
    for file_entry in fs::read_dir(cert_dir)? {
        let file = file_entry?;
        let f = File::open(file.path())?;
        let mut rd = BufReader::new(f);
        let _ = root_store.add_pem_file(&mut rd).map(|_| ()).map_err(|()| {
            Error::new(
                ErrorKind::InvalidData,
                "could not load PEM file".to_string(),
            )
        });
    }
    Ok(())
}

fn read_pvt_key(
    user_manager: UserManager,
    key_file: tedge_config::FilePath,
) -> Result<rustls_0_19::PrivateKey, ConnectError> {
    // Become BROKER_USER to read the private key
    let _user_guard = user_manager.become_user(tedge_users::BROKER_USER)?;
    parse_pkcs8_key(key_file.clone()).or_else(|_| parse_rsa_key(key_file))
}

fn parse_pkcs8_key(
    key_file: tedge_config::FilePath,
) -> Result<rustls_0_19::PrivateKey, ConnectError> {
    let f = File::open(&key_file)?;
    let mut key_reader = BufReader::new(f);
    match pkcs8_private_keys(&mut key_reader) {
        Ok(key) if !key.is_empty() => Ok(key[0].clone()),
        _ => Err(ConnectError::UnknownPrivateKeyFormat),
    }
}

fn parse_rsa_key(
    key_file: tedge_config::FilePath,
) -> Result<rustls_0_19::PrivateKey, ConnectError> {
    let f = File::open(&key_file)?;
    let mut key_reader = BufReader::new(f);
    match rsa_private_keys(&mut key_reader) {
        Ok(key) if !key.is_empty() => Ok(key[0].clone()),
        _ => Err(ConnectError::UnknownPrivateKeyFormat),
    }
}

fn read_cert_chain(
    cert_file: tedge_config::FilePath,
) -> Result<Vec<rustls_0_19::Certificate>, ConnectError> {
    let f = File::open(cert_file)?;
    let mut cert_reader = BufReader::new(f);
    certs(&mut cert_reader).map_err(|_| ConnectError::RumqttcCertificate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn parse_private_rsa_key() {
        let key = concat!(
            "-----BEGIN RSA PRIVATE KEY-----\n",
            "MC4CAQ\n",
            "-----END RSA PRIVATE KEY-----"
        );
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(key.as_bytes()).unwrap();
        let result = parse_rsa_key(temp_file.path().into()).unwrap();
        let pvt_key = rustls_0_19::PrivateKey(vec![48, 46, 2, 1]);
        assert_eq!(result, pvt_key);
    }

    #[test]
    fn parse_private_pkcs8_key() {
        let key = concat! {
        "-----BEGIN PRIVATE KEY-----\n",
        "MC4CAQ\n",
        "-----END PRIVATE KEY-----"};
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(key.as_bytes()).unwrap();
        let result = parse_pkcs8_key(temp_file.path().into()).unwrap();
        let pvt_key = rustls_0_19::PrivateKey(vec![48, 46, 2, 1]);
        assert_eq!(result, pvt_key);
    }

    #[test]
    fn parse_supported_key() {
        let user_manager = UserManager::new();
        let key = concat!(
            "-----BEGIN RSA PRIVATE KEY-----\n",
            "MC4CAQ\n",
            "-----END RSA PRIVATE KEY-----"
        );
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(key.as_bytes()).unwrap();
        let parsed_key = read_pvt_key(user_manager, temp_file.path().into()).unwrap();
        let expected_pvt_key = rustls_0_19::PrivateKey(vec![48, 46, 2, 1]);
        assert_eq!(parsed_key, expected_pvt_key);
    }

    #[test]
    fn parse_unsupported_key() {
        let user_manager = UserManager::new();
        let key = concat!(
            "-----BEGIN DSA PRIVATE KEY-----\n",
            "MC4CAQ\n",
            "-----END DSA PRIVATE KEY-----"
        );
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(key.as_bytes()).unwrap();
        let err = read_pvt_key(user_manager, temp_file.path().into()).unwrap_err();
        assert!(matches!(err, ConnectError::UnknownPrivateKeyFormat));
    }
}
