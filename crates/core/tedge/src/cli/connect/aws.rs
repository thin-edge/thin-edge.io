use super::command::bridge_health_topic;
use super::command::is_bridge_health_up_message;
use crate::cli::RESPONSE_TIMEOUT;
use crate::ConnectError;
use crate::DeviceStatus;
use anyhow::anyhow;
use rumqttc::Event;
use rumqttc::Incoming;
use rumqttc::Outgoing;
use rumqttc::Packet;
use rumqttc::QoS::AtLeastOnce;
use tedge_config::tedge_toml::ProfileName;
use tedge_config::TEdgeConfig;

pub async fn check_device_status_aws(
    tedge_config: &TEdgeConfig,
    profile: Option<&ProfileName>,
) -> Result<DeviceStatus, ConnectError> {
    let aws_config = tedge_config.aws.try_get(profile)?;
    let topic_prefix = &aws_config.bridge.topic_prefix;
    let aws_topic_pub_check_connection = format!("{topic_prefix}/test-connection");
    let aws_topic_sub_check_connection = format!("{topic_prefix}/connection-success");
    let built_in_bridge_health = bridge_health_topic(topic_prefix, tedge_config)
        .unwrap()
        .name;
    const CLIENT_ID: &str = "check_connection_aws";
    const REGISTRATION_PAYLOAD: &[u8] = b"";

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
        .subscribe(&aws_topic_sub_check_connection, AtLeastOnce)
        .await?;

    let mut err = None;
    loop {
        match event_loop.poll().await {
            Ok(Event::Incoming(Packet::SubAck(_))) => {
                // We are ready to get the response, hence send the request
                client
                    .publish(
                        &aws_topic_pub_check_connection,
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
                if response.topic == aws_topic_sub_check_connection {
                    // We got a response
                    break;
                } else if is_bridge_health_up_message(&response, &built_in_bridge_health) {
                    // Built in bridge is now up, republish the message in case it was never received by the bridge
                    client
                        .publish(
                            &aws_topic_pub_check_connection,
                            AtLeastOnce,
                            false,
                            REGISTRATION_PAYLOAD,
                        )
                        .await?;
                }
            }
            Ok(Event::Outgoing(Outgoing::PingReq)) => {
                // No messages have been received for a while
                err = Some(if acknowledged {
                    anyhow!("Didn't receive response from AWS")
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
        match event_loop.poll().await {
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
            .context("Failed to verify device is connected to AWS")
            .into()),
    }
}
