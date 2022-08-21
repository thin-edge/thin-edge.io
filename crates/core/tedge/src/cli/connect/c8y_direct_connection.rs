use super::{BridgeConfig, ConnectError};
use certificate::parse_root_certificate::*;
use rumqttc::{self, Client, Event, Incoming, MqttOptions, Outgoing, Packet, QoS, Transport};

// Connect directly to the c8y cloud over mqtt and publish device create message.
pub fn create_device_with_direct_connection(
    bridge_config: &BridgeConfig,
    device_type: &str,
) -> Result<(), ConnectError> {
    const DEVICE_ALREADY_EXISTS: &[u8] = b"41,100,Device already existing";
    const DEVICE_CREATE_ERROR_TOPIC: &str = "s/e";

    let address = bridge_config.address.clone();
    let host: Vec<&str> = address.split(':').collect();

    let mut mqtt_options = MqttOptions::new(bridge_config.remote_clientid.clone(), host[0], 8883);
    mqtt_options.set_keep_alive(std::time::Duration::from_secs(5));

    let mut tls_config = create_tls_config();

    load_root_certs(
        &mut tls_config.root_store,
        bridge_config.bridge_root_cert_path.clone().into(),
    )?;

    let pvt_key = read_pvt_key(bridge_config.bridge_keyfile.clone().into())?;
    let cert_chain = read_cert_chain(bridge_config.bridge_certfile.clone().into())?;

    let _ = tls_config.set_single_client_cert(cert_chain, pvt_key);
    mqtt_options.set_transport(Transport::tls_with_config(tls_config.into()));

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
