use crate::cli::bridge_health_topic;
use crate::cli::RESPONSE_TIMEOUT;
use crate::ConnectError;
use crate::DeviceStatus;
use anyhow::anyhow;
use rumqttc::Event;
use rumqttc::Incoming;
use rumqttc::Outgoing;
use rumqttc::Packet;
use rumqttc::QoS::AtLeastOnce;
use tedge_config::tedge_toml::mapper_config::AzMapperSpecificConfig;
use tedge_config::tedge_toml::ProfileName;
use tedge_config::TEdgeConfig;

// Here We check the az device twin properties over mqtt to check if connection has been open.
// First the mqtt client will subscribe to a topic az/$iothub/twin/res/#, listen to the
// device twin property output.
// Empty payload will be published to az/$iothub/twin/GET/?$rid=1, here 1 is request ID.
// The result will be published by the iothub on the az/$iothub/twin/res/{status}/?$rid={request id}.
// Here if the status is 200 then it's success.
pub(crate) async fn check_device_status_azure(
    tedge_config: &TEdgeConfig,
    profile: Option<&ProfileName>,
) -> Result<DeviceStatus, ConnectError> {
    let az_config = tedge_config
        .mapper_config::<AzMapperSpecificConfig>(&profile)
        .await?;
    let topic_prefix = &az_config.bridge.topic_prefix;
    let built_in_bridge_health = bridge_health_topic(topic_prefix, tedge_config).name;
    let azure_topic_device_twin_downstream = format!(r##"{topic_prefix}/twin/res/#"##);
    let azure_topic_device_twin_upstream = format!(r#"{topic_prefix}/twin/GET/?$rid=1"#);
    const CLIENT_ID: &str = "check_connection_az";
    const REGISTRATION_PAYLOAD: &[u8] = b"";
    const REGISTRATION_OK: &str = "200";

    let mut mqtt_options = tedge_config
        .mqtt_config()?
        .with_session_name(CLIENT_ID)
        .rumqttc_options()?;

    mqtt_options.set_keep_alive(RESPONSE_TIMEOUT);

    let (client, mut event_loop) = rumqttc::AsyncClient::new(mqtt_options, 10);
    let mut acknowledged = false;

    if tedge_config.mqtt.bridge.built_in {
        client
            .subscribe(&built_in_bridge_health, AtLeastOnce)
            .await?;
    }
    client
        .subscribe(azure_topic_device_twin_downstream, AtLeastOnce)
        .await?;

    let mut err = None;
    loop {
        match event_loop.poll().await {
            Ok(Event::Incoming(Packet::SubAck(_))) => {
                // We are ready to get the response, hence send the request
                client
                    .publish(
                        &azure_topic_device_twin_upstream,
                        AtLeastOnce,
                        false,
                        REGISTRATION_PAYLOAD,
                    )
                    .await?;
            }
            Ok(Event::Incoming(Packet::PubAck(_))) => {
                // The request has been sent
                acknowledged = true;
            }
            Ok(Event::Incoming(Packet::Publish(response))) => {
                if response.topic.contains(REGISTRATION_OK) {
                    // We got a response
                    break;
                } else if response.topic == built_in_bridge_health {
                    client
                        .publish(
                            &azure_topic_device_twin_upstream,
                            AtLeastOnce,
                            false,
                            REGISTRATION_PAYLOAD,
                        )
                        .await?;
                } else {
                    break;
                }
            }
            Ok(Event::Outgoing(Outgoing::PingReq)) => {
                // No messages have been received for a while
                err = Some(if acknowledged {
                    anyhow!("Didn't receive a response from Azure")
                } else {
                    anyhow!("Local MQTT publish has timed out")
                });
                break;
            }
            Ok(Event::Incoming(Incoming::Disconnect)) => {
                err = Some(anyhow!(
                    "Client was disconnected from mosquitto during connection check"
                ));
                break;
            }
            Err(e) => {
                err = Some(
                    anyhow::Error::from(e)
                        .context("Failed to connect to mosquitto for connection check"),
                );
                break;
            }
            _ => {}
        }
    }

    // Cleanly disconnect client
    client.disconnect().await?;
    loop {
        let event = event_loop.poll().await;
        match event {
            Ok(Event::Outgoing(Outgoing::Disconnect)) | Err(_) => break,
            _ => {}
        }
    }

    match err {
        None => Ok(DeviceStatus::AlreadyExists),
        // In Cumulocity we connect directly first to create a device so we know we can connect so
        // we return `DeviceStatus::Unknown` when we can't check its status, but here we can fail to
        // even connect because we're connecting through the bridge and haven't connected directly
        // prior
        Some(err) => Err(err
            .context("Failed to verify device is connected to Azure")
            .into()),
    }
}
