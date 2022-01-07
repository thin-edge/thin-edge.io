use rumqttc::{
    self, certs, pkcs8_private_keys, AsyncClient, Client, Event, Incoming, MqttOptions, QoS,
    Transport,
};
use rustls_0_19::ClientConfig;
use std::{error::Error, fs::File, io::BufReader, thread, time::Duration};
use tokio::{task, time};

// #[derive(thiserror::Error, Debug)]
// pub enum LocalError {
//     NoValidCertInChain,
// }

pub fn create_device_with_direct_connection() {
    println!(
        "......Directly connects to c8y to create a new device before creating the bridge...."
    );
    let mut mqtt_options =
        MqttOptions::new("directcon", "thin-edge-io.eu-latest.cumulocity.com", 8883);
    mqtt_options.set_keep_alive(std::time::Duration::from_secs(10));

    // To customise TLS configuration we create a rustls ClientConfig and set it up how we want.
    let mut client_config = ClientConfig::new();
    // Use rustls-native-certs to load root certificates from the operating system.
    client_config.root_store =
        rustls_native_certs::load_native_certs().expect("Failed to load platform certificates.");
    if client_config.root_store.is_empty() {
        dbg!("store is empty");
    } else {
        let f = File::open("/etc/tedge/device-certs/tedge-private-key.pem").unwrap();
        let mut key_reader = BufReader::new(f);
        let key_chain: Vec<rustls_0_19::PrivateKey> = pkcs8_private_keys(&mut key_reader).unwrap();
        //dbg!(&key_chain);
        let key = key_chain.first().unwrap().clone();
        // Get the first key. Error if it's not valid
        // let key = match key_chain.first().unwrap()[0] {
        //     Some(k) => k.clone(),
        //     None => return Err(LocalError::NoValidCertInChain),
        // };

        let f = File::open("/etc/tedge/device-certs/tedge-certificate.pem").unwrap();
        let mut cert_reader = BufReader::new(f);
        let cert_chain: Vec<rustls_0_19::Certificate> = certs(&mut cert_reader).unwrap();

        let _ = client_config.set_single_client_cert(cert_chain, key);
    }

    mqtt_options.set_transport(Transport::tls_with_config(client_config.into()));

    let (mut client, mut connection) = Client::new(mqtt_options, 10);
    thread::spawn(move || requests(&mut client));

    for (i, notification) in connection.iter().enumerate() {
        match notification.unwrap(){
           Event::Incoming(Incoming::Publish(p)) => {
                println!("Topic: {}, Payload: {:?}", p.topic, p.payload)
            }
            Event::Incoming(i) => {
                println!("Incoming = {:?}", i);
            }
            Event::Outgoing(o) => println!("Outgoing = {:?}", o),
           e => {
                println!("Errors = {:?}", e);
            }
        }
        // println!("{:#?}. Notification = {:?}", i, notification.unwrap());
    }
}

fn requests(client: &mut Client) {
    client.subscribe("s/e", QoS::AtMostOnce).unwrap();
    client.subscribe("s/ds", QoS::AtMostOnce).unwrap();

    let payload: String = String::from("100,directcon,thin-edge.io");
    client
        .publish("s/us", QoS::ExactlyOnce, false, payload.as_bytes())
        .unwrap();

    thread::sleep(Duration::from_secs(1));
}
