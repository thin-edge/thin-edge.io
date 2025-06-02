use super::ConnectError;
use crate::bridge::BridgeConfig;
use crate::bridge::BridgeLocation;
use crate::cli::bridge_health_topic;
use crate::cli::connect::CONNECTION_TIMEOUT;
use crate::cli::connect::RESPONSE_TIMEOUT;
use crate::cli::is_bridge_health_up_message;
use crate::DeviceStatus;
use anyhow::anyhow;
use anyhow::bail;
use anyhow::Context as _;
use base64::prelude::*;
use c8y_api::smartrest::message::get_smartrest_template_id;
use c8y_api::smartrest::message_ids::JWT_TOKEN;
use certificate::parse_root_certificate::create_tls_config_without_client_cert;
use rumqttc::tokio_rustls::rustls::AlertDescription;
use rumqttc::tokio_rustls::rustls::CertificateError;
use rumqttc::tokio_rustls::rustls::Error;
use rumqttc::AsyncClient;
use rumqttc::ConnectionError;
use rumqttc::Event;
use rumqttc::Incoming;
use rumqttc::MqttOptions;
use rumqttc::Outgoing;
use rumqttc::Packet;
use rumqttc::QoS;
use rumqttc::QoS::AtLeastOnce;
use rumqttc::TlsError;
use rumqttc::Transport;
use tedge_config::models::auth_method::AuthType;
use tedge_config::tedge_toml::MqttAuthConfigCloudBroker;
use tedge_config::tedge_toml::ProfileName;
use tedge_config::TEdgeConfig;

const CONNECTION_ERROR_CONTEXT: &str = "Connection error while creating device in Cumulocity";

// Connect directly to the c8y cloud over mqtt and publish device create message.
pub async fn create_device_with_direct_connection(
    bridge_config: &BridgeConfig,
    device_type: &str,
    // TODO: put into general authentication struct
    mqtt_auth_config: MqttAuthConfigCloudBroker,
) -> anyhow::Result<()> {
    const DEVICE_ALREADY_EXISTS: &[u8] = b"41,100,Device already existing";
    const DEVICE_CREATE_ERROR_TOPIC: &str = "s/e";

    let address = bridge_config.address.clone();

    let mut mqtt_options = MqttOptions::new(
        bridge_config.remote_clientid.clone(),
        address.host().to_string(),
        address.port().into(),
    );
    mqtt_options.set_keep_alive(std::time::Duration::from_secs(5));

    let tls_config = if bridge_config.auth_type == AuthType::Basic {
        mqtt_options.set_credentials(
            bridge_config
                .remote_username
                .clone()
                .expect("username must be set to use basic auth"),
            bridge_config
                .remote_password
                .clone()
                .expect("password must be set to use basic auth"),
        );
        create_tls_config_without_client_cert(&bridge_config.bridge_root_cert_path)?
    } else {
        mqtt_auth_config.to_rustls_client_config()?
    };
    mqtt_options.set_transport(Transport::tls_with_config(tls_config.into()));

    // Only connect via proxy if built-in bridge is enabled since the proxy is
    // ignored when using mosquitto bridge
    if bridge_config.bridge_location == BridgeLocation::BuiltIn {
        if let Some(proxy) = &bridge_config.proxy {
            mqtt_options.set_proxy(proxy.0.clone());
        }
    }

    let (mut client, mut eventloop) = AsyncClient::new(mqtt_options, 10);
    eventloop
        .network_options
        .set_connection_timeout(CONNECTION_TIMEOUT.as_secs());

    client
        .subscribe(DEVICE_CREATE_ERROR_TOPIC, QoS::AtLeastOnce)
        .await?;

    let mut device_create_try: usize = 0;
    loop {
        match eventloop.poll().await {
            Ok(Event::Incoming(Packet::SubAck(_) | Packet::PubAck(_) | Packet::PubComp(_))) => {
                publish_device_create_message(
                    &mut client,
                    &bridge_config.remote_clientid.clone(),
                    device_type,
                )
                .await?;
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
                    )
                    .await?;
                    device_create_try += 1;
                } else {
                    // No messages have been received for a while
                    break;
                }
            }
            Ok(Event::Incoming(Incoming::Disconnect)) => {
                bail!("Unexpectedly disconnected from Cumulocity while attempting to create device")
            }
            Err(ConnectionError::Io(err)) if err.kind() == std::io::ErrorKind::InvalidData => {
                if let Some(Error::AlertReceived(alert_description)) = err
                    .get_ref()
                    .and_then(|custom_err| custom_err.downcast_ref::<Error>())
                {
                    if let AlertDescription::CertificateUnknown = alert_description {
                        // Either the device cert is not uploaded to c8y or
                        // another cert is set in device.cert_path
                        bail!("The device certificate is not trusted by Cumulocity. Upload the certificate using `tedge cert upload c8y`");
                    } else if let AlertDescription::HandshakeFailure = alert_description {
                        // Non-paired private key is set in device.key_path
                        bail!(
                            "The private key is not paired with the certificate. Check your 'device.key_path'."
                        );
                    }
                }
                return Err(err).context(CONNECTION_ERROR_CONTEXT);
            }
            Err(ConnectionError::Tls(TlsError::Io(err)))
                if err.kind() == std::io::ErrorKind::InvalidData =>
            {
                match err
                    .get_ref()
                    .and_then(|custom_err| custom_err.downcast_ref::<Error>())
                {
                    Some(Error::InvalidCertificate(CertificateError::UnknownIssuer)) => {
                        bail!("Cumulocity certificate is not trusted by the device. Check your 'c8y.root_cert_path'.");
                    }
                    _ => return Err(err).context(CONNECTION_ERROR_CONTEXT),
                }
            }
            Err(err) => return Err(err).context(CONNECTION_ERROR_CONTEXT),
            _ => {}
        }
    }

    bail!("Timed-out attempting to create device in Cumulocity")
}

// Check the connection by using the jwt token retrieval over the mqtt.
// If successful in getting the jwt token '71,xxxxx', the connection is established.
pub(crate) async fn check_device_status_c8y(
    tedge_config: &TEdgeConfig,
    c8y_profile: Option<&ProfileName>,
) -> Result<DeviceStatus, ConnectError> {
    let c8y_config = tedge_config.c8y.try_get(c8y_profile)?;

    // TODO: Use SmartREST1 to check connection
    if c8y_config
        .auth_method
        .is_basic(&c8y_config.credentials_path)
    {
        return Ok(DeviceStatus::AlreadyExists);
    }

    let prefix = &c8y_config.bridge.topic_prefix;
    let built_in_bridge_health = bridge_health_topic(prefix, tedge_config).unwrap().name;
    let c8y_topic_builtin_jwt_token_downstream = format!("{prefix}/s/dat");
    let c8y_topic_builtin_jwt_token_upstream = format!("{prefix}/s/uat");
    const CLIENT_ID: &str = "check_connection_c8y";

    let mut mqtt_options = tedge_config
        .mqtt_config()?
        .with_session_name(CLIENT_ID)
        .with_clean_session(true)
        .rumqttc_options()?;

    mqtt_options.set_keep_alive(RESPONSE_TIMEOUT);

    let (client, mut event_loop) = rumqttc::AsyncClient::new(mqtt_options, 10);
    event_loop
        .network_options
        .set_connection_timeout(CONNECTION_TIMEOUT.as_secs());
    let mut acknowledged = false;

    let built_in_bridge = tedge_config.mqtt.bridge.built_in;
    if built_in_bridge {
        client
            .subscribe(&built_in_bridge_health, AtLeastOnce)
            .await?;
    }
    client
        .subscribe(&c8y_topic_builtin_jwt_token_downstream, AtLeastOnce)
        .await?;

    let mut err = None;
    loop {
        match event_loop.poll().await {
            Ok(Event::Incoming(Packet::SubAck(_))) => {
                // We are ready to get the response, hence send the request
                client
                    .publish(
                        &c8y_topic_builtin_jwt_token_upstream,
                        rumqttc::QoS::AtMostOnce,
                        false,
                        "",
                    )
                    .await?;
            }
            Ok(Event::Incoming(Packet::PubAck(_))) => {
                // The request has been sent
                acknowledged = true;
            }
            Ok(Event::Incoming(Packet::Publish(response))) => {
                if response.topic == c8y_topic_builtin_jwt_token_downstream {
                    // We got a response
                    let response = std::str::from_utf8(&response.payload).unwrap();
                    let message_id = get_smartrest_template_id(response);
                    if message_id.parse() == Ok(JWT_TOKEN) {
                        break;
                    }
                } else if is_bridge_health_up_message(&response, &built_in_bridge_health) {
                    client
                        .publish(
                            &c8y_topic_builtin_jwt_token_upstream,
                            rumqttc::QoS::AtMostOnce,
                            false,
                            "",
                        )
                        .await?;
                }
            }
            Ok(Event::Outgoing(Outgoing::PingReq)) => {
                // No messages have been received for a while
                err = Some(if acknowledged {
                    anyhow!("Didn't receive response from Cumulocity")
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
        // The request has been sent but without a response
        Some(_) if acknowledged => Ok(DeviceStatus::Unknown),
        // The request has not even been sent
        Some(err) => Err(err
            .context("Failed to verify device is connected to Cumulocity")
            .into()),
    }
}

async fn publish_device_create_message(
    client: &mut AsyncClient,
    device_id: &str,
    device_type: &str,
) -> Result<(), ConnectError> {
    use c8y_api::smartrest::message_ids::DEVICE_CREATION;
    const DEVICE_CREATE_PUBLISH_TOPIC: &str = "s/us";
    client
        .publish(
            DEVICE_CREATE_PUBLISH_TOPIC,
            QoS::ExactlyOnce,
            false,
            format!("{DEVICE_CREATION},{},{}", device_id, device_type).as_bytes(),
        )
        .await?;
    Ok(())
}

pub(crate) async fn get_connected_c8y_url(
    tedge_config: &TEdgeConfig,
    c8y_prefix: Option<&str>,
) -> Result<String, ConnectError> {
    let prefix = &tedge_config.c8y.try_get(c8y_prefix)?.bridge.topic_prefix;
    let c8y_topic_builtin_jwt_token_upstream = format!("{prefix}/s/uat");
    let c8y_topic_builtin_jwt_token_downstream = format!("{prefix}/s/dat");
    const CLIENT_ID: &str = "get_jwt_token_c8y";

    let mut mqtt_options = tedge_config
        .mqtt_config()?
        .with_session_name(CLIENT_ID)
        .with_clean_session(true)
        .rumqttc_options()?;
    mqtt_options.set_keep_alive(RESPONSE_TIMEOUT);

    let (client, mut event_loop) = rumqttc::AsyncClient::new(mqtt_options, 10);
    event_loop
        .network_options
        .set_connection_timeout(CONNECTION_TIMEOUT.as_secs());
    let mut acknowledged = false;
    let mut c8y_url: Option<String> = None;

    client
        .subscribe(c8y_topic_builtin_jwt_token_downstream, AtLeastOnce)
        .await?;
    let mut err = None;

    loop {
        match event_loop.poll().await {
            Ok(Event::Incoming(Packet::SubAck(_))) => {
                // We are ready to get the response, hence send the request
                client
                    .publish(
                        &c8y_topic_builtin_jwt_token_upstream,
                        rumqttc::QoS::AtMostOnce,
                        false,
                        "",
                    )
                    .await?;
            }
            Ok(Event::Incoming(Packet::PubAck(_))) => {
                // The request has been sent
                acknowledged = true;
            }
            Ok(Event::Incoming(Packet::Publish(response))) => {
                // We got a response
                let token = String::from_utf8(response.payload.to_vec()).unwrap();
                let connected_url = decode_jwt_token(token.as_str())?;
                c8y_url = Some(connected_url);
                break;
            }
            Ok(Event::Outgoing(Outgoing::PingReq)) => {
                let rest = if acknowledged {
                    // The request has been sent but without a response
                    "The request has been sent, however, no response was received"
                } else {
                    // The request has not even been sent
                    "Make sure mosquitto is running."
                };
                // No messages have been received for a while
                err = Some(anyhow!("Timed out obtaining Cumulocity URL. {rest}"));
                break;
            }
            Ok(Event::Incoming(Incoming::Disconnect)) => {
                err = Some(anyhow!(
                    "Client was disconnected from mosquitto while obtaining Cumulocity URL."
                ));
                break;
            }
            Err(e) => {
                err = Some(anyhow::Error::from(e).context(
                    "Client failed to connect to mosquitto while obtaining Cumulocity URL",
                ));
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

    if let Some(c8y_url) = c8y_url {
        Ok(c8y_url)
    } else {
        Err(err.map_or(ConnectError::TimeoutElapsedError, Into::into))
    }
}

pub(crate) fn decode_jwt_token(token: &str) -> Result<String, ConnectError> {
    // JWT token format: <header>.<payload>.<signature>. Thus, we want only <payload>.
    let payload = token
        .split_terminator('.')
        .nth(1)
        .ok_or(ConnectError::InvalidJWTToken {
            token: token.to_string(),
            reason: "JWT token format must be <header>.<payload>.<signature>.".to_string(),
        })?;

    let decoded =
        BASE64_URL_SAFE_NO_PAD
            .decode(payload)
            .map_err(|_| ConnectError::InvalidJWTToken {
                token: token.to_string(),
                reason: "Cannot decode the payload of JWT token by Base64 without padding."
                    .to_string(),
            })?;

    let json: serde_json::Value =
        serde_json::from_slice(decoded.as_slice()).map_err(|_| ConnectError::InvalidJWTToken {
            token: token.to_string(),
            reason: "The payload of JWT token is not JSON.".to_string(),
        })?;

    let tenant_url = json["iss"].as_str().ok_or(ConnectError::InvalidJWTToken {
        token: token.to_string(),
        reason: "The JSON decoded from JWT token doesn't contain 'iss' field.".to_string(),
    })?;

    Ok(tenant_url.to_string())
}

#[cfg(test)]
mod test {
    use super::*;
    use test_case::test_case;

    #[test]
    fn check_decode_valid_jwt_token() {
        let token = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJqdGkiOm51bGwsImlzcyI6InRlc3QuY3VtdWxvY2l0eS5jb20iLCJhdWQiOiJ0ZXN0LmN1bXVsb2NpdHkuY29tIiwic3ViIjoiZGV2aWNlX3Rlc3QwMDA1IiwidGNpIjoiZGV2aWNlX3Rva2VuX2NvbmZpZyIsImlhdCI6MTYzODQ0Mjk5NywibmJmIjoxNjM4NDQyOTk3LCJleHAiOjE2Mzg0NDY1OTcsInRmYSI6ZmFsc2UsInRlbiI6InQzMTcwNDgiLCJ4c3JmVG9rZW4iOiJLc2VBVUZBTGF1aUplZFFNR2ZzRiJ9.JUYtU9FVWlOWUPJXawFzKNiHD4HoEEWmvKdU1k9L2UF2ofRA2zAdcLH4mxaaspt4suyyZbPL6cS6c9MROG3YCsnqle2NSoYw8mxqncFECWsDS8lwCRTG4402iPTETfWpo9uXw2pFryBoJMAvNzt1qsXXn8EXSYxjzgj0YyxSANypm7PL1kMaprdLuUML_9Cwxf7Z6CRyWkZWWmnQ3lYgV5KMGW7HznkkqcmUCvuXKrHhVL5RkmzE1WyL4ndpGEPFEv9VYmEvFYA8wVHSuw5iVZIFp5lQldDdy_8U-N80xnf3fqZ6Q_wnVm8cga77vIgcf9zK5rSCdehvolM48uM4_w";
        let expected_url = "test.cumulocity.com";
        assert_eq!(decode_jwt_token(token).unwrap(), expected_url.to_string());
    }

    #[test]
    fn check_decode_jwt_token_missing_base64_padding() {
        // JWTs don't pad base64-encoded strings to make them more compact. This
        // JWT has a 215 byte payload, so if our parsing disallows non-padded
        // input (base64 that isn't a multiple of 4 bytes long), we will fail on this valid JWT
        let token = "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJpc3MiOiJ0ZXN0LmN1bXVsb2NpdHkuY29tIiwiaWF0IjoxNzQyMjI3NjY4LCJleHAiOjE3NDIyMzEyNjgsImF1ZCI6InRlc3QuY3VtdWxvY2l0eS5jb20iLCJzdWIiOiJkZXZpY2VfdGVzdDAwNSIsIm5iZiI6IjE3NDIyMjc2NjgiLCJ0Y2kiOiJkZXZpY2VfdG9rZW5fY29uZmlnIn0.JgoTORxZk8LN51e9-gHzfpr59JlaIT5oHFXuGQxP2zY";
        let expected_url = "test.cumulocity.com";
        assert_eq!(decode_jwt_token(token).unwrap(), expected_url.to_string());
    }

    #[test_case(
    "dGVzdC5jdW11bG9jaXR5LmNvbQ",
    "The JWT token received from Cumulocity is invalid.\n\
    Token: dGVzdC5jdW11bG9jaXR5LmNvbQ\n\
    Reason: JWT token format must be <header>.<payload>.<signature>."
    ; "not jwt token"
    )]
    #[test_case(
    "aaa.bbb.ccc",
    "The JWT token received from Cumulocity is invalid.\n\
    Token: aaa.bbb.ccc\n\
    Reason: Cannot decode the payload of JWT token by Base64 without padding."
    ; "payload is not base64 encoded"
    )]
    #[test_case(
    "aaa.eyJpc3MiOiJ0ZXN0LmN1bXVsb2NpdHkuY29tIn0=.ccc",
    "The JWT token received from Cumulocity is invalid.\n\
    Token: aaa.eyJpc3MiOiJ0ZXN0LmN1bXVsb2NpdHkuY29tIn0=.ccc\n\
    Reason: Cannot decode the payload of JWT token by Base64 without padding."
    ; "payload has base64 padding"
    )]
    #[test_case(
    "aaa.dGVzdC5jdW11bG9jaXR5LmNvbQ.ccc",
    "The JWT token received from Cumulocity is invalid.\n\
    Token: aaa.dGVzdC5jdW11bG9jaXR5LmNvbQ.ccc\n\
    Reason: The payload of JWT token is not JSON."
    ; "payload is not json"
    )]
    #[test_case(
    "aaa.eyJqdGkiOm51bGwsImF1ZCI6InRlc3QuY3VtdWxvY2l0eS5jb20iLCJzdWIiOiJkZXZpY2VfdGVzdDAwMDUiLCJ0Y2kiOiJkZXZpY2VfdG9rZW5fY29uZmlnIiwiaWF0IjoxNjM4NDQyOTk3LCJuYmYiOjE2Mzg0NDI5OTcsImV4cCI6MTYzODQ0NjU5NywidGZhIjpmYWxzZSwidGVuIjoidDMxNzA0OCIsInhzcmZUb2tlbiI6IktzZUFVRkFMYXVpSmVkUU1HZnNGIn0.ccc",
    "The JWT token received from Cumulocity is invalid.\n\
    Token: aaa.eyJqdGkiOm51bGwsImF1ZCI6InRlc3QuY3VtdWxvY2l0eS5jb20iLCJzdWIiOiJkZXZpY2VfdGVzdDAwMDUiLCJ0Y2kiOiJkZXZpY2VfdG9rZW5fY29uZmlnIiwiaWF0IjoxNjM4NDQyOTk3LCJuYmYiOjE2Mzg0NDI5OTcsImV4cCI6MTYzODQ0NjU5NywidGZhIjpmYWxzZSwidGVuIjoidDMxNzA0OCIsInhzcmZUb2tlbiI6IktzZUFVRkFMYXVpSmVkUU1HZnNGIn0.ccc\n\
    Reason: The JSON decoded from JWT token doesn't contain 'iss' field."
    ; "payload is json but not contains iss field"
    )]
    fn check_decode_invalid_jwt_token(input: &str, expected_error_msg: &str) {
        match decode_jwt_token(input) {
            Ok(_) => panic!("This test should result in an error"),
            Err(err) => {
                let error_msg = format!("{}", err);
                assert_eq!(error_msg, expected_error_msg)
            }
        }
    }
}
