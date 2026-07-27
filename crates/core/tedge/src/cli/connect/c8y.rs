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
use c8y_api::smartrest::message_ids::GET_DEVICE_MANAGED_OBJECT_ID;
use c8y_api::smartrest::message_ids::GET_DEVICE_MANAGED_OBJECT_ID_RESPONSE;
use c8y_api::smartrest::message_ids::JWT_TOKEN;
use certificate::parse_root_certificate::create_tls_config_without_client_cert;
use rumqttc::tokio_rustls::rustls::AlertDescription;
use rumqttc::tokio_rustls::rustls::CertificateError;
use rumqttc::tokio_rustls::rustls::Error;
use rumqttc::tokio_rustls::rustls::InvalidMessage;
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
use tedge_config::tedge_toml::mapper_config::C8yMapperConfig;
use tedge_config::tedge_toml::mapper_config::C8yMapperSpecificConfig;
use tedge_config::tedge_toml::MqttAuthConfigCloudBroker;
use tedge_config::tedge_toml::ProfileName;
use tedge_config::TEdgeConfig;
use tracing::debug;

/// Reported for an error raised before the broker accepted the MQTT connection
const HANDSHAKE_ERROR_CONTEXT: &str = "Connection error while connecting to Cumulocity";

/// Reported for an error raised once the broker had accepted the MQTT connection
const CONNECTION_ERROR_CONTEXT: &str = "Connection error while creating device in Cumulocity";

/// Says which stage an otherwise unexplained error came from, so that a bare I/O error at least
/// tells the user whether the connection was ever established
fn error_context(connected: bool) -> &'static str {
    if connected {
        CONNECTION_ERROR_CONTEXT
    } else {
        HANDSHAKE_ERROR_CONTEXT
    }
}

/// The message `rustls` reports when it has no room left to buffer an incoming message
const BUFFER_FULL_MESSAGE: &str = "message buffer full";

// Connect directly to the c8y cloud over mqtt and publish device create message.
pub async fn create_device_with_direct_connection(
    bridge_config: &BridgeConfig,
    device_type: &str,
    // TODO: put into general authentication struct
    mqtt_auth_config: MqttAuthConfigCloudBroker,
) -> anyhow::Result<()> {
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

    // Tracks whether the TLS handshake is behind us, so that errors can be attributed to the right
    // stage. A ConnAck is the first event the broker sends us, so it is the earliest point at which
    // we know the handshake succeeded
    let mut connected = false;

    loop {
        match eventloop.poll().await {
            Ok(Event::Incoming(Packet::ConnAck(connack))) => {
                debug!(
                    "Received ConnAck ({:?}), session_present={:?}",
                    connack.code, connack.session_present
                );
                connected = true;
                // Connection established, publish device creation message
                publish_device_create_message(
                    &mut client,
                    &bridge_config.remote_clientid.clone(),
                    device_type,
                )
                .await?;
            }
            Ok(Event::Incoming(Packet::PubAck(_))) => {
                debug!("Received PubAck");
                // Device creation message acknowledged by the cloud
                return Ok(());
            }
            Ok(Event::Incoming(Incoming::Disconnect)) => {
                debug!("Received Disconnect");
                bail!("Unexpectedly disconnected from Cumulocity while attempting to create device")
            }
            Ok(Event::Incoming(Incoming::PingResp)) => {
                debug!("Received PingResp");
            }
            Ok(Event::Outgoing(Outgoing::PingReq)) => {
                // No acknowledgment received for device creation message even after 5s (keep alive interval)
                bail!("Timed-out waiting for device creation acknowledgment from Cumulocity")
            }
            Err(ConnectionError::Io(err) | ConnectionError::Tls(TlsError::Io(err))) => {
                match diagnose_tls_error(&err, &address.host().to_string(), connected) {
                    Some(explanation) => bail!("{explanation}"),
                    None => return Err(err).context(error_context(connected)),
                }
            }
            Err(err) => return Err(err).context(error_context(connected)),
            _ => {}
        }
    }
}

/// Recognises `rustls` refusing to buffer any more of an over-large message
///
/// `rustls` gives up without constructing a `rustls::Error`, so there is nothing to downcast to
/// and the message has to be matched instead. A test below checks this still matches what the
/// current `rustls` version reports. The same message is used whether or not a handshake is in
/// progress, and the two cases have different size limits, so the caller must establish which of
/// them applies
fn is_buffer_full(err: &std::io::Error) -> bool {
    err.get_ref()
        .is_some_and(|inner| inner.to_string() == BUFFER_FULL_MESSAGE)
}

/// Explains a TLS error in terms of what the user can do about it
///
/// Returns `None` for anything unrecognised, leaving the caller to report the error as it is.
/// `connected` tells us whether the broker has accepted the MQTT connection, and hence whether the
/// TLS handshake is behind us
fn diagnose_tls_error(err: &std::io::Error, host: &str, connected: bool) -> Option<String> {
    if err.kind() != std::io::ErrorKind::InvalidData {
        return None;
    }

    match err
        .get_ref()
        .and_then(|custom_err| custom_err.downcast_ref::<Error>())
    {
        // Either the device cert is not uploaded to c8y or
        // another cert is set in device.cert_path
        Some(Error::AlertReceived(AlertDescription::CertificateUnknown)) => Some(
            "The device certificate is not trusted by Cumulocity. Upload the certificate using `tedge cert upload c8y`".into(),
        ),
        // Non-paired private key is set in device.key_path
        Some(Error::AlertReceived(AlertDescription::HandshakeFailure)) => Some(
            "The private key is not paired with the certificate. Check your 'device.key_path'.".into(),
        ),
        Some(Error::InvalidCertificate(CertificateError::UnknownIssuer)) => Some(
            "Cumulocity certificate is not trusted by the device. Check your 'c8y.root_cert_path'.".into(),
        ),
        // A single handshake message that announces more than `rustls` will buffer
        Some(Error::InvalidMessage(InvalidMessage::HandshakePayloadTooLarge)) if !connected => {
            Some(oversized_handshake(host))
        }
        // A handshake that fills the buffer before any one message completes. Only a handshake
        // can reach the larger of the two buffer limits, so both diagnoses hold only while the
        // connection is still being established
        _ if !connected && is_buffer_full(err) => Some(oversized_handshake(host)),
        _ => None,
    }
}

/// Explains a handshake too large for `rustls` to accept, however it broke the limit
fn oversized_handshake(host: &str) -> String {
    format!(
        "Cumulocity sent a TLS handshake that is too large for thin-edge.io to accept (over 64 KB).\n\
        The likely cause is the server listing the client certificates it will accept, \
        which it should not be sending.\n\
        Whatever the cause, this is not something that can be fixed on this device. \
        Please report it to Cumulocity support, quoting the host {host}."
    )
}

// Check the connection by using the jwt token retrieval over the mqtt.
// If successful in getting the jwt token '71,xxxxx', the connection is established.
pub(crate) async fn check_device_status_c8y(
    tedge_config: &TEdgeConfig,
    c8y_profile: Option<&ProfileName>,
) -> Result<DeviceStatus, ConnectError> {
    let c8y_config = tedge_config.mapper_config::<C8yMapperSpecificConfig>(&c8y_profile)?;
    let prefix = &c8y_config.bridge.topic_prefix;
    let built_in_bridge = tedge_config.mqtt.bridge.built_in;
    let bridge_health_topic = bridge_health_topic(prefix, tedge_config).name;

    let (downstream_topic, upstream_topic, payload) = if c8y_config
        .cloud_specific
        .auth_method
        .is_basic(&c8y_config.cloud_specific.credentials_path)
    {
        (
            format!("{prefix}/s/ds"),
            format!("{prefix}/s/us"),
            GET_DEVICE_MANAGED_OBJECT_ID.to_string(),
        )
    } else {
        (
            format!("{prefix}/s/dat"),
            format!("{prefix}/s/uat"),
            "".to_string(),
        )
    };

    const CLIENT_ID: &str = "check_connection_c8y";

    let mut mqtt_options = tedge_config
        .mqtt_config()?
        .with_session_prefix(CLIENT_ID)
        .rumqttc_options()?;

    mqtt_options.set_keep_alive(RESPONSE_TIMEOUT);

    let (client, mut event_loop) = rumqttc::AsyncClient::new(mqtt_options, 10);
    event_loop
        .network_options
        .set_connection_timeout(CONNECTION_TIMEOUT.as_secs());
    let mut acknowledged = false;

    client.subscribe(&bridge_health_topic, AtLeastOnce).await?;
    client.subscribe(&downstream_topic, AtLeastOnce).await?;

    let mut bridge_connected = false;

    let mut err = None;
    loop {
        match event_loop.poll().await {
            Ok(Event::Incoming(Packet::PubAck(_))) => {
                // The request has been sent
                acknowledged = true;
            }
            Ok(Event::Incoming(Packet::Publish(response))) => {
                if response.topic == downstream_topic {
                    // We got a response
                    let response = std::str::from_utf8(&response.payload).unwrap();
                    let message_id = get_smartrest_template_id(response);
                    if message_id.parse() == Ok(GET_DEVICE_MANAGED_OBJECT_ID_RESPONSE)
                        || message_id.parse() == Ok(JWT_TOKEN)
                    {
                        bridge_connected = true;
                        break;
                    }
                } else if is_bridge_health_up_message(
                    &response,
                    &bridge_health_topic,
                    built_in_bridge,
                ) {
                    client
                        .publish(
                            &upstream_topic,
                            rumqttc::QoS::AtMostOnce,
                            false,
                            payload.clone(),
                        )
                        .await?;
                }
            }
            Ok(Event::Outgoing(Outgoing::PingReq)) => {
                // No messages have been received for a while
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

    if !bridge_connected {
        err = Some(anyhow!("Connection to Cumulocity is not healthy"));
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
    let payload = format!("{DEVICE_CREATION},{device_id},{device_type}");
    debug!(
        "Registering device in Cumulocity. topic={DEVICE_CREATE_PUBLISH_TOPIC}, payload={payload}"
    );
    client
        .publish(
            DEVICE_CREATE_PUBLISH_TOPIC,
            QoS::AtLeastOnce,
            false,
            payload.as_bytes(),
        )
        .await?;
    Ok(())
}

pub(crate) async fn get_connected_c8y_url(
    tedge_config: &TEdgeConfig,
    c8y_config: &C8yMapperConfig,
) -> Result<String, ConnectError> {
    let prefix = &c8y_config.bridge.topic_prefix;
    let c8y_topic_builtin_jwt_token_upstream = format!("{prefix}/s/uat");
    let c8y_topic_builtin_jwt_token_downstream = format!("{prefix}/s/dat");
    const CLIENT_ID: &str = "get_jwt_token_c8y";

    let mut mqtt_options = tedge_config
        .mqtt_config()?
        .with_session_prefix(CLIENT_ID)
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
    use rumqttc::tokio_rustls::rustls::ClientConfig;
    use rumqttc::tokio_rustls::rustls::ClientConnection;
    use rumqttc::tokio_rustls::rustls::RootCertStore;
    use std::io::Cursor;
    use std::sync::Arc;
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

    #[test]
    fn blames_cumulocity_for_a_handshake_too_large_to_buffer() {
        let explanation = diagnose_tls_error(&oversized_handshake_error(), HOST, false)
            .expect("an oversized handshake is diagnosed");

        assert!(
            explanation.contains("too large"),
            "should say what went wrong: {explanation}"
        );
        assert!(
            explanation.contains("Cumulocity support") && explanation.contains(HOST),
            "should say who to report it to: {explanation}"
        );
    }

    /// The other way `rustls` refuses an over-large handshake: rather than filling the buffer, a
    /// single message announces more than it will ever accept
    #[test]
    fn blames_cumulocity_for_a_handshake_message_that_announces_too_much() {
        let err = std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            Error::InvalidMessage(InvalidMessage::HandshakePayloadTooLarge),
        );

        let explanation = diagnose_tls_error(&err, HOST, false)
            .expect("an over-large handshake message is diagnosed");

        assert_eq!(explanation, oversized_handshake(HOST));
    }

    #[test]
    fn does_not_blame_the_handshake_for_an_over_large_message_after_connecting() {
        let err = std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            Error::InvalidMessage(InvalidMessage::HandshakePayloadTooLarge),
        );

        assert_eq!(diagnose_tls_error(&err, HOST, true), None);
    }

    /// `rustls` reports a full buffer the same way once the handshake is done, but then it means a
    /// single over-long record rather than an over-long handshake, so the diagnosis must not apply
    #[test]
    fn does_not_blame_the_handshake_for_a_full_buffer_after_connecting() {
        assert_eq!(
            diagnose_tls_error(&oversized_handshake_error(), HOST, true),
            None
        );
    }

    #[test]
    fn advises_uploading_the_certificate_when_cumulocity_does_not_trust_it() {
        let err = alert(AlertDescription::CertificateUnknown);

        let explanation =
            diagnose_tls_error(&err, HOST, false).expect("a rejected certificate is diagnosed");

        assert!(
            explanation.contains("tedge cert upload c8y"),
            "{explanation}"
        );
    }

    #[test]
    fn advises_checking_the_key_path_when_the_key_does_not_match_the_certificate() {
        let err = alert(AlertDescription::HandshakeFailure);

        let explanation =
            diagnose_tls_error(&err, HOST, false).expect("an unpaired private key is diagnosed");

        assert!(explanation.contains("device.key_path"), "{explanation}");
    }

    #[test]
    fn advises_checking_the_root_cert_path_when_the_device_does_not_trust_cumulocity() {
        let err = std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            Error::InvalidCertificate(CertificateError::UnknownIssuer),
        );

        let explanation =
            diagnose_tls_error(&err, HOST, false).expect("an untrusted server is diagnosed");

        assert!(explanation.contains("c8y.root_cert_path"), "{explanation}");
    }

    #[test]
    fn leaves_an_unrecognised_error_to_be_reported_verbatim() {
        let err = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "connection refused");

        assert_eq!(diagnose_tls_error(&err, HOST, false), None);
    }

    #[test]
    fn error_context_says_whether_the_connection_was_ever_established() {
        assert_ne!(error_context(true), error_context(false));
    }

    const HOST: &str = "example.cumulocity.com";

    /// The largest handshake flight `rustls` will buffer, as a guard against denial-of-service
    ///
    /// Mirrors `MAX_HANDSHAKE_SIZE` in `rustls`, which is not publicly exported
    const MAX_HANDSHAKE_SIZE: usize = 0xffff;

    /// Wraps a TLS alert the way `rustls` surfaces it through `read_tls`
    fn alert(description: AlertDescription) -> std::io::Error {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            Error::AlertReceived(description),
        )
    }

    /// Drives a real [`ClientConnection`] into rejecting an over-large handshake
    ///
    /// Going through `rustls` rather than hand-building the error means an upgrade that changes
    /// how the refusal is reported fails these tests instead of silently losing the diagnosis
    fn oversized_handshake_error() -> std::io::Error {
        // Must be called before any use of `ClientConfig::builder()`
        let _ = rumqttc::tokio_rustls::rustls::crypto::ring::default_provider().install_default();

        let config = ClientConfig::builder()
            .with_root_certificates(RootCertStore::empty())
            .with_no_client_auth();
        let mut connection =
            ClientConnection::new(Arc::new(config), "example.com".try_into().unwrap())
                .expect("a client connection can be created");
        connection
            .write_tls(&mut std::io::sink())
            .expect("the client hello can be flushed");

        let mut flight = Cursor::new(oversized_handshake_flight());
        loop {
            match connection.read_tls(&mut flight) {
                Ok(0) => panic!("rustls buffered the whole flight without complaining"),
                Ok(_) => connection
                    .process_new_packets()
                    .expect("the partial flight is not rejected on its content"),
                Err(err) => break err,
            };
        }
    }

    /// Frames a single `Certificate` handshake message into plaintext TLS records
    ///
    /// The message declares just under [`MAX_HANDSHAKE_SIZE`] bytes of payload, so once its own
    /// header and the record headers are counted, the flight can never fit in the buffer and
    /// `rustls` gives up part way through
    fn oversized_handshake_flight() -> Vec<u8> {
        const HANDSHAKE_RECORD: u8 = 22;
        const TLS_1_2_VERSION: [u8; 2] = [0x03, 0x03];
        const MAX_RECORD_PAYLOAD: usize = 16_384;
        const CERTIFICATE_MESSAGE: u8 = 11;

        let payload_len = MAX_HANDSHAKE_SIZE - 1;
        let mut message = vec![CERTIFICATE_MESSAGE];
        // A handshake message length is 3 bytes wide
        message.extend_from_slice(&(payload_len as u32).to_be_bytes()[1..]);
        message.resize(message.len() + payload_len, 0);

        let mut flight = Vec::new();
        for chunk in message.chunks(MAX_RECORD_PAYLOAD) {
            flight.push(HANDSHAKE_RECORD);
            flight.extend_from_slice(&TLS_1_2_VERSION);
            flight.extend_from_slice(&(chunk.len() as u16).to_be_bytes());
            flight.extend_from_slice(chunk);
        }
        flight
    }
}
