use rumqttc::{
    self, certs, pkcs8_private_keys, Client, Event, Incoming, MqttOptions, QoS, Transport,
};
use rustls_0_19::ClientConfig;
use std::{fs::File, io::BufReader, thread, time::Duration};

use super::{BridgeConfig, ConnectError};

pub fn create_device_with_direct_connection(
    bridge_config: &BridgeConfig,
) -> Result<(), ConnectError> {
    let address = bridge_config.address.clone();
    let host: Vec<&str> = address.split(":").collect();

    let mut mqtt_options = MqttOptions::new(bridge_config.remote_clientid.clone(), host[0], 8883);
    mqtt_options.set_keep_alive(std::time::Duration::from_secs(10));

    // To customise TLS configuration we create a rustls ClientConfig and set it up how we want.
    let mut client_config = ClientConfig::new();
    // Use rustls-native-certs to load root certificates from the operating system.
    client_config.root_store =
        rustls_native_certs::load_native_certs().expect("Failed to load platform certificates.");
    if client_config.root_store.is_empty() {
        dbg!("store is empty");
    } else {
        let f = File::open(bridge_config.bridge_keyfile.clone())?;
        let mut key_reader = BufReader::new(f);
        let key_chain: Vec<rustls_0_19::PrivateKey> = pkcs8_private_keys(&mut key_reader).unwrap();
        //dbg!(&key_chain);
        let key = key_chain.first().unwrap().clone();

        let f = File::open(bridge_config.bridge_certfile.clone())?;
        let mut cert_reader = BufReader::new(f);
        let cert_chain: Vec<rustls_0_19::Certificate> = certs(&mut cert_reader).unwrap();

        let _ = client_config.set_single_client_cert(cert_chain, key);
    }

    mqtt_options.set_transport(Transport::tls_with_config(client_config.into()));

    let (mut client, mut connection) = Client::new(mqtt_options, 10);
    thread::spawn(move || requests(&mut client));

    for (_i, notification) in connection.iter().enumerate() {
        match notification.unwrap() {
            Event::Incoming(Incoming::Publish(_p)) => {
                // println!("Topic: {}, Payload: {:?}", p.topic, p.payload);
                // Validate the string (41,100,Device already existing) before breaking
                break;
            }
            Event::Incoming(_i) => {}
            Event::Outgoing(_o) => {}
        }
    }
    Ok(())
}

fn requests(client: &mut Client) -> Result<(), ConnectError> {
    client.subscribe("s/e", QoS::AtMostOnce).unwrap();

    let payload: String = String::from("100,directcon,thin-edge.io");
    client.publish("s/us", QoS::ExactlyOnce, false, payload.as_bytes())?;

    // Sleep a while before sending another device create request to check the device already created or not.
    thread::sleep(Duration::from_secs(3));

    client.publish("s/us", QoS::ExactlyOnce, false, payload.as_bytes())?;

    Ok(())
}
