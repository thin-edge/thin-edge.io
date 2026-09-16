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
use base64::prelude::*;
use c8y_api::smartrest::message::get_smartrest_template_id;
use c8y_api::smartrest::message_ids::GET_DEVICE_MANAGED_OBJECT_ID;
use c8y_api::smartrest::message_ids::GET_DEVICE_MANAGED_OBJECT_ID_RESPONSE;
use c8y_api::smartrest::message_ids::JWT_TOKEN;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use certificate::parse_root_certificate::create_tls_config_without_client_cert;
use certificate::PemCertificate;
use certificate::ValidityStatus;
use rumqttc::tokio_rustls::rustls::AlertDescription;
use rumqttc::tokio_rustls::rustls::CertificateError;
use rumqttc::tokio_rustls::rustls::Error;
use rumqttc::tokio_rustls::rustls::InconsistentKeys;
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
use std::time::Duration;
use tedge_config::models::auth_method::AuthType;
use tedge_config::tedge_toml::mapper_config::C8yMapperConfig;
use tedge_config::tedge_toml::mapper_config::C8yMapperSpecificConfig;
use tedge_config::tedge_toml::MqttAuthConfigCloudBroker;
use tedge_config::tedge_toml::PrivateKeyType;
use tedge_config::tedge_toml::ProfileName;
use tedge_config::TEdgeConfig;
use tracing::debug;

/// Reported for an error raised before the broker accepted the MQTT connection
const HANDSHAKE_ERROR_CONTEXT: &str = "Connection error while connecting to Cumulocity";

/// Reported for an error raised once the broker had accepted the MQTT connection
const CONNECTION_ERROR_CONTEXT: &str = "Connection error while creating device in Cumulocity";

/// How far a connection attempt had got when it failed
///
/// Several readings of an error hold only while the handshake is still going on, and the same
/// error means something else afterwards
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    /// The TLS handshake is still in progress
    Handshaking,
    /// The broker has accepted the MQTT connection, so the handshake succeeded
    Connected,
}

/// Says which stage an otherwise unexplained error came from, so that a bare I/O error at least
/// tells the user whether the connection was ever established
fn error_context(stage: Stage) -> &'static str {
    match stage {
        Stage::Connected => CONNECTION_ERROR_CONTEXT,
        Stage::Handshaking => HANDSHAKE_ERROR_CONTEXT,
    }
}

/// The message `rustls` reports when it has no room left to buffer an incoming message
const BUFFER_FULL_MESSAGE: &str = "message buffer full";

// Connect directly to the c8y cloud over mqtt and publish device create message.
pub async fn create_device_with_direct_connection(
    bridge_config: &BridgeConfig,
    profile: Option<&ProfileName>,
    device_type: &str,
    // TODO: put into general authentication struct
    mqtt_auth_config: MqttAuthConfigCloudBroker,
) -> anyhow::Result<()> {
    let address = bridge_config.address.clone();
    let host = address.host().to_string();
    // A proxy is only used by the built-in bridge.
    let proxy = bridge_config
        .proxy
        .as_ref()
        .filter(|_| bridge_config.bridge_location == BridgeLocation::BuiltIn)
        .map(|proxy| format!("{}:{}", proxy.0.addr, proxy.0.port));
    let connection = ConnectionDetails::new(
        &host,
        &bridge_config.remote_clientid,
        profile,
        &mqtt_auth_config,
        bridge_config.auth_type,
        proxy,
    );

    let mut mqtt_options = MqttOptions::new(
        bridge_config.remote_clientid.clone(),
        host.clone(),
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
        mqtt_auth_config.to_rustls_client_config().map_err(|err| {
            match classify_config_error(&err) {
                // The explanation is what the user reads; the error it was drawn from stays as its
                // source, so nothing is lost if the reading is wrong
                Some(failure) => err.context(failure.explain(&connection, None)),
                None => err,
            }
        })?
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
    let mut stage = Stage::Handshaking;

    loop {
        match eventloop.poll().await {
            Ok(Event::Incoming(Packet::ConnAck(connack))) => {
                debug!(
                    "Received ConnAck ({:?}), session_present={:?}",
                    connack.code, connack.session_present
                );
                stage = Stage::Connected;
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
                let Some(failure) = classify_tls_error(&err, &connection, stage) else {
                    return Err(anyhow!("{err:#}\n\n{}", error_context(stage)));
                };

                let validity = match (failure, connection.certificate()) {
                    (TlsFailure::DeviceCertificateRejected, Some(credentials)) => {
                        certificate_validity(&credentials.cert_path).await
                    }
                    _ => None,
                };

                return Err(anyhow!(
                    "{err:#}\n\n{}",
                    failure.explain(&connection, validity)
                ));
            }
            Err(err) => {
                let err: anyhow::Error = err.into();
                if let Some(err2 @ std::io::Error { .. }) = err.root_cause().downcast_ref() {
                    if let Some(Error::AlertReceived(AlertDescription::HandshakeFailure)) =
                        err2.get_ref().and_then(|e| e.downcast_ref())
                    {
                        let failure = TlsFailure::UnpairedPrivateKey;
                        return Err(anyhow!("{err:#}\n\n{}", failure.explain(&connection, None)));
                    }
                }

                return Err(anyhow!("{err:#}\n\n{}", error_context(stage)));
            }
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

/// The files and settings a connection attempt used, so that a diagnosis can name them
///
/// Nothing secret is held here: the PKCS#11 PIN is never read, and neither is the key URI, which
/// may carry a `pin-value` attribute of its own. The messages name the setting that holds the URI
/// instead of quoting its value
struct ConnectionDetails {
    /// Host of the Cumulocity MQTT endpoint
    host: String,

    /// Identity the device claims, which is the common name of its certificate
    device_id: String,

    /// Prefix of this connection's `tedge config` keys: `c8y`, or `c8y.profiles.<name>`
    config_prefix: String,

    root_cert_path: Utf8PathBuf,

    client: ClientAuth,

    /// Address of the proxy the connection is made through
    proxy: Option<String>,
}

/// What the connection offered to authenticate itself
enum ClientAuth {
    /// A certificate and its private key
    Certificate(ClientCredentials),

    /// A username and password, with no certificate of the device's own
    UsernameAndPassword,
}

/// The certificate and private key a connection presented to Cumulocity
struct ClientCredentials {
    cert_path: Utf8PathBuf,
    key: KeyLocation,
}

/// Where a private key lives, which decides both what to print and which setting to name
enum KeyLocation {
    File(Utf8PathBuf),
    /// A PKCS#11 token, chosen by `<prefix>.device.key_uri` rather than `<prefix>.device.key_path`
    Token,
}

impl ConnectionDetails {
    /// Reads the connection details from the authentication config the connection itself uses
    ///
    /// The paths come from `mqtt_auth_config` rather than from `bridge_config`, as those are the
    /// files actually presented: when reconnecting to validate a renewed certificate, the caller
    /// swaps in the new certificate there
    fn new(
        host: &str,
        device_id: &str,
        profile: Option<&ProfileName>,
        mqtt_auth_config: &MqttAuthConfigCloudBroker,
        auth_type: AuthType,
        proxy: Option<String>,
    ) -> Self {
        let client = &mqtt_auth_config.client;
        Self {
            host: host.to_owned(),
            device_id: device_id.to_owned(),
            config_prefix: match profile {
                None => "c8y".into(),
                Some(profile) => format!("c8y.profiles.{profile}"),
            },
            root_cert_path: mqtt_auth_config.ca_path.clone(),
            client: match auth_type {
                // The certificate settings still hold paths under basic auth, but the connection
                // does not send them, so they are not part of what it presented
                AuthType::Basic => ClientAuth::UsernameAndPassword,
                AuthType::Certificate => ClientAuth::Certificate(ClientCredentials {
                    cert_path: client.cert_file.clone(),
                    key: match &client.private_key {
                        PrivateKeyType::File(path) => KeyLocation::File(path.clone()),
                        PrivateKeyType::Cryptoki(_) => KeyLocation::Token,
                    },
                }),
            },
            proxy,
        }
    }

    /// Names the `tedge config` key that sets one of this connection's values
    fn config_key(&self, key: &str) -> String {
        format!("{}.{key}", self.config_prefix)
    }

    /// The certificate this connection presented, if certificate authentication is in use
    fn certificate(&self) -> Option<&ClientCredentials> {
        match &self.client {
            ClientAuth::Certificate(client) => Some(client),
            ClientAuth::UsernameAndPassword => None,
        }
    }
}

/// What a failed connection attempt was, as far as it can be established from the error
///
/// Deciding this is kept apart from wording it, so that what the code concluded can be checked
/// without reading prose, and the prose can be checked without producing a TLS failure
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TlsFailure {
    /// A private key that does not belong to the certificate configured with it
    UnpairedPrivateKey,

    /// The server rejected the device certificate
    DeviceCertificateRejected,

    /// A handshake the remote endpoint refused without saying why
    HandshakeRejected,

    /// The device rejected the certificate Cumulocity presented
    UntrustedServer,

    /// A handshake larger than `rustls` will accept, however it broke the limit
    OversizedHandshake,
}

/// Reads an error from building the client configuration as one of the failures we can explain
///
/// `rustls` compares a private key read from a file with the public key in the certificate sent
/// alongside it, and refuses the configuration if they do not match. That happens before anything
/// is sent, which is why an unpaired key is established here rather than guessed at from a
/// handshake that failed
fn classify_config_error(err: &anyhow::Error) -> Option<TlsFailure> {
    // `CertificateError::CertParse` is transparent, so the `rustls` error it holds is not a link in
    // the chain of its own: it has to be reached through the variant rather than downcast to
    match err
        .chain()
        .find_map(|cause| cause.downcast_ref::<certificate::CertificateError>())?
    {
        certificate::CertificateError::CertParse(Error::InconsistentKeys(
            InconsistentKeys::KeyMismatch,
        )) => Some(TlsFailure::UnpairedPrivateKey),
        _ => None,
    }
}

/// Reads a TLS error as one of the failures we can explain
///
/// Returns `None` for anything unrecognised, leaving the caller to report the error as it is
fn classify_tls_error(
    err: &std::io::Error,
    connection: &ConnectionDetails,
    stage: Stage,
) -> Option<TlsFailure> {
    if err.kind() != std::io::ErrorKind::InvalidData {
        return None;
    }

    match err
        .get_ref()
        .and_then(|custom_err| custom_err.downcast_ref::<Error>())
    {
        // Only classify this as a device-certificate rejection when the connection sent one.
        Some(Error::AlertReceived(AlertDescription::CertificateUnknown)) => connection
            .certificate()
            .map(|_| TlsFailure::DeviceCertificateRejected),
        Some(Error::AlertReceived(AlertDescription::HandshakeFailure)) => {
            Some(TlsFailure::HandshakeRejected)
        }
        Some(Error::InvalidCertificate(CertificateError::UnknownIssuer)) => {
            Some(TlsFailure::UntrustedServer)
        }
        // A single handshake message that announces more than `rustls` will buffer
        Some(Error::InvalidMessage(InvalidMessage::HandshakePayloadTooLarge))
            if stage == Stage::Handshaking =>
        {
            Some(TlsFailure::OversizedHandshake)
        }
        // A handshake that fills the buffer before any one message completes. Only a handshake
        // can reach the larger of the two buffer limits, so both readings hold only while the
        // connection is still being established
        _ if stage == Stage::Handshaking && is_buffer_full(err) => {
            Some(TlsFailure::OversizedHandshake)
        }
        _ => None,
    }
}

/// Reads how much life a certificate has left, as far as the file can be read at all
///
/// A certificate that cannot be read leaves the failure to be explained without it, rather than
/// replacing one unexplained failure with another
async fn certificate_validity(path: &Utf8Path) -> Option<ValidityStatus> {
    let pem = tokio::fs::read_to_string(path).await.ok()?;

    PemCertificate::from_pem_string(&pem)
        .ok()?
        .still_valid()
        .ok()
}

impl TlsFailure {
    /// Explains the failure in terms of what the user can do about it
    fn explain(self, connection: &ConnectionDetails, validity: Option<ValidityStatus>) -> String {
        match self {
            Self::DeviceCertificateRejected => rejected_certificate(connection, validity),
            Self::UnpairedPrivateKey => unpaired_private_key(connection),
            Self::HandshakeRejected => handshake_rejected(connection),
            Self::UntrustedServer => untrusted_server(connection),
            Self::OversizedHandshake => oversized_handshake(connection),
        }
    }
}

/// Renders a duration in the units a certificate's lifetime is set in
fn in_days(duration: Duration) -> String {
    match duration.as_secs() / (24 * 60 * 60) {
        0 => "less than a day".to_owned(),
        1 => "1 day".to_owned(),
        days => format!("{days} days"),
    }
}

/// Explains Cumulocity rejecting the device certificate, using its validity when readable.
fn rejected_certificate(
    connection: &ConnectionDetails,
    validity: Option<ValidityStatus>,
) -> String {
    let Some(credentials) = connection.certificate() else {
        return handshake_rejected(connection);
    };
    let cert_path = &credentials.cert_path;
    let cert_key = connection.config_key("device.cert_path");
    let ConnectionDetails {
        host, device_id, ..
    } = connection;
    let certificate = format!(
        "The certificate sent was {cert_path} (set by '{cert_key}'), which identifies this device \
        as '{device_id}'."
    );

    match validity {
        Some(ValidityStatus::Expired { since }) => {
            let ago = in_days(since);
            format!(
                "The device certificate expired {ago} ago, so {host} rejected it.\n\
                \n\
                {certificate}\n\
                \n\
                Renew the certificate with `tedge cert renew --ca self-signed`, then upload the \
                renewed certificate with `tedge cert upload c8y`."
            )
        }
        Some(ValidityStatus::NotValidYet { valid_in }) => {
            let ahead = in_days(valid_in);
            format!(
                "The device certificate is not valid until {ahead} from now, so {host} rejected \
                it.\n\
                \n\
                {certificate}\n\
                \n\
                Check the clock on this device and the service that keeps it in sync. If the \
                clock is correct, renew the certificate with `tedge cert renew --ca self-signed` \
                and upload it with `tedge cert upload c8y`."
            )
        }
        Some(ValidityStatus::Valid { .. }) | None => format!(
            "Cumulocity did not recognise the device certificate.\n\
            \n\
            {certificate}\n\
            \n\
            Check that this is the certificate registered as trusted in the tenant at {host}. If \
            it is not, upload it with `tedge cert upload c8y`."
        ),
    }
}

/// Explains rustls rejecting a certificate and file-based private key before connecting.
fn unpaired_private_key(connection: &ConnectionDetails) -> String {
    let Some(client) = connection.certificate() else {
        return handshake_rejected(connection);
    };
    let cert_path = &client.cert_path;
    let cert_key = connection.config_key("device.cert_path");
    let (key, advice) = match &client.key {
        KeyLocation::File(key_path) => (
            format!(
                "{key_path} (from '{}')",
                connection.config_key("device.key_path")
            ),
            "`tedge cert create` always writes a matching pair, so they normally only come apart if \
            one of those settings now points somewhere else, or if one of the two files has since \
            been replaced on its own.\n\
            \n\
            Run `tedge cert renew --ca self-signed` to issue a certificate for the key this device \
            holds, then `tedge cert upload c8y` to add it to the tenant. That keeps the device \
            identity, where `tedge cert create` would refuse to overwrite what is already there."
                .to_owned(),
        ),
        KeyLocation::Token => (
            format!(
                "an HSM private key (selected by '{}')",
                connection.config_key("device.key_uri")
            ),
            format!(
                "So check that '{cert_key}' names the certificate that was issued for the key held \
                in the HSM, and not another certificate."
            ),
        ),
    };

    format!(
        "The configured private key does not belong to the device certificate.\n\
        \n\
        This connection is configured to use:\n\
        \x20 certificate: {cert_path} (from '{cert_key}')\n\
        \x20 private key: {key}\n\
        \n\
        These two have to be a matching pair: the certificate has to be the one issued for that \
        exact key.\n\
        \n\
        {advice}"
    )
}

fn handshake_rejected(connection: &ConnectionDetails) -> String {
    let host = &connection.host;
    let mut explanation = format!(
        "{host} refused the TLS handshake without giving a reason.\n\
        \n\
        Check that `{host}` is the correct MQTT endpoint for this tenant. If it is, ask the \
        Cumulocity administrator to check the tenant's TLS configuration and server logs."
    );

    if let Some(ClientCredentials {
        cert_path,
        key: KeyLocation::Token,
    }) = connection.certificate()
    {
        explanation.push_str(&format!(
            "\n\
            \n\
            This connection uses an HSM private key. Check that `{}` points to the certificate \
            issued for the key selected by `{}`. The certificate supplied was {cert_path}.",
            connection.config_key("device.cert_path"),
            connection.config_key("device.key_uri")
        ));
    }

    if let Some(proxy) = &connection.proxy {
        explanation.push_str(&format!(
            "\n\
            \n\
            This connection goes through proxy `{proxy}`, which may also be involved. Include its \
            address when asking your network administrator or Cumulocity administrator for help."
        ));
    }

    explanation
}

/// Explains the device refusing the certificate presented by the remote endpoint
fn untrusted_server(connection: &ConnectionDetails) -> String {
    let root_cert_key = connection.config_key("root_cert_path");
    let ConnectionDetails {
        host,
        root_cert_path,
        ..
    } = connection;
    format!(
        "The device does not trust the certificate presented by {host}: it is signed by a \
        certificate authority the device does not know.\n\
        \n\
        This check is about the remote endpoint's identity, not the device's — the certificate and key \
        created by `tedge cert create` play no part in it. The authorities the device trusts are \
        read from {root_cert_path} (set by '{root_cert_key}').\n\
        \n\
        Check that:\n\
        \x20 * {root_cert_path} is the right file or directory — on most Linux systems the \
        system's own CA certificates are in /etc/ssl/certs\n\
        \x20 * the authority that signed the certificate presented on this connection is among \
        those it holds"
    )
}

fn oversized_handshake(connection: &ConnectionDetails) -> String {
    let host = &connection.host;
    let mut explanation = format!(
        "Cumulocity sent a TLS handshake larger than the 64 KB thin-edge.io can accept.\n\
        \n\
        The likely cause is an unnecessarily large `certificate_authorities` list. This must be \
        corrected on the Cumulocity side; changing this device's certificate or configuration \
        will not fix it.\n\
        \n\
        Contact Cumulocity support and quote the MQTT host `{host}`."
    );

    if let Some(proxy) = &connection.proxy {
        explanation.push_str(&format!(
            "\n\
            \n\
            This connection goes through proxy `{proxy}`, which may be responsible for the \
            oversized handshake. Ask whoever manages that proxy to check it; if it is only \
            forwarding the connection, contact Cumulocity support and quote both `{host}` and \
            `{proxy}`."
        ));
    }

    explanation
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
    use tedge_config::tedge_toml::MqttAuthClientConfigCloudBroker;
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
    fn reads_a_full_buffer_during_the_handshake_as_an_oversized_handshake() {
        assert_eq!(
            classify_tls_error(
                &oversized_handshake_error(),
                &connection(),
                Stage::Handshaking
            ),
            Some(TlsFailure::OversizedHandshake)
        );
    }

    /// The other way `rustls` refuses an over-large handshake: rather than filling the buffer, a
    /// single message announces more than it will ever accept
    #[test]
    fn reads_a_handshake_message_that_announces_too_much_as_an_oversized_handshake() {
        let err = std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            Error::InvalidMessage(InvalidMessage::HandshakePayloadTooLarge),
        );

        assert_eq!(
            classify_tls_error(&err, &connection(), Stage::Handshaking),
            Some(TlsFailure::OversizedHandshake)
        );
    }

    #[test]
    fn does_not_read_an_over_large_message_after_connecting_as_a_handshake_failure() {
        let err = std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            Error::InvalidMessage(InvalidMessage::HandshakePayloadTooLarge),
        );

        assert_eq!(
            classify_tls_error(&err, &connection(), Stage::Connected),
            None
        );
    }

    /// `rustls` reports a full buffer the same way once the handshake is done, but then it means a
    /// single over-long record rather than an over-long handshake, so the reading must not apply
    #[test]
    fn does_not_read_a_full_buffer_after_connecting_as_a_handshake_failure() {
        assert_eq!(
            classify_tls_error(
                &oversized_handshake_error(),
                &connection(),
                Stage::Connected
            ),
            None
        );
    }

    #[test]
    fn reads_a_certificate_unknown_alert_as_a_certificate_cumulocity_does_not_hold() {
        assert_eq!(
            classify_tls_error(
                &alert(AlertDescription::CertificateUnknown),
                &connection(),
                Stage::Handshaking,
            ),
            Some(TlsFailure::DeviceCertificateRejected)
        );
    }

    #[test]
    fn reads_a_handshake_failure_as_a_bare_refusal_when_the_key_is_on_a_token() {
        let connection = ConnectionDetails {
            client: ClientAuth::Certificate(ClientCredentials {
                cert_path: CERT_PATH.into(),
                key: KeyLocation::Token,
            }),
            ..connection()
        };

        assert_eq!(
            classify_tls_error(
                &alert(AlertDescription::HandshakeFailure),
                &connection,
                Stage::Handshaking
            ),
            Some(TlsFailure::HandshakeRejected)
        );
    }

    /// A key read from a file has already been compared with its certificate, so a handshake that
    /// fails afterwards cannot be blamed on the two not belonging together
    #[test]
    fn reads_a_handshake_failure_as_a_bare_refusal_when_the_key_is_a_file() {
        assert_eq!(
            classify_tls_error(
                &alert(AlertDescription::HandshakeFailure),
                &connection(),
                Stage::Handshaking
            ),
            Some(TlsFailure::HandshakeRejected)
        );
    }

    #[test]
    fn does_not_blame_a_certificate_that_was_never_sent() {
        let connection = ConnectionDetails {
            client: ClientAuth::UsernameAndPassword,
            ..connection()
        };

        assert_eq!(
            classify_tls_error(
                &alert(AlertDescription::CertificateUnknown),
                &connection,
                Stage::Handshaking
            ),
            None
        );
        assert_eq!(
            classify_tls_error(
                &alert(AlertDescription::HandshakeFailure),
                &connection,
                Stage::Handshaking
            ),
            Some(TlsFailure::HandshakeRejected)
        );
    }

    /// A certificate the tenant may well hold, and would reject all the same
    #[tokio::test]
    async fn reads_the_validity_of_an_expired_certificate() {
        let dir = tempfile::tempdir().unwrap();
        let (cert_path, _) = write_selfsigned_pair_valid_for(dir.path(), "device-a", 0);
        let connection = ConnectionDetails {
            client: ClientAuth::Certificate(ClientCredentials {
                cert_path,
                key: KeyLocation::File(KEY_PATH.into()),
            }),
            ..connection()
        };

        assert!(matches!(
            certificate_validity(&connection.certificate().unwrap().cert_path).await,
            Some(ValidityStatus::Expired { .. })
        ));
    }

    /// The state a device with a clock set behind the real date ends up in
    #[tokio::test]
    async fn reads_the_validity_of_a_certificate_that_starts_later() {
        let dir = tempfile::tempdir().unwrap();
        let cert_path = write_certificate_valid_from(dir.path(), "device-a", 2);
        let connection = ConnectionDetails {
            client: ClientAuth::Certificate(ClientCredentials {
                cert_path,
                key: KeyLocation::File(KEY_PATH.into()),
            }),
            ..connection()
        };

        assert!(matches!(
            certificate_validity(&connection.certificate().unwrap().cert_path).await,
            Some(ValidityStatus::NotValidYet { .. })
        ));
    }

    /// A certificate in date leaves the alert to say what it says, as does one that cannot be read
    #[tokio::test]
    async fn reads_the_validity_of_an_in_date_certificate() {
        let dir = tempfile::tempdir().unwrap();
        let (cert_path, _) = write_selfsigned_pair_valid_for(dir.path(), "device-a", 365);
        let connection = ConnectionDetails {
            client: ClientAuth::Certificate(ClientCredentials {
                cert_path,
                key: KeyLocation::File(KEY_PATH.into()),
            }),
            ..connection()
        };

        assert!(matches!(
            certificate_validity(&connection.certificate().unwrap().cert_path).await,
            Some(ValidityStatus::Valid { .. })
        ));
    }

    #[test]
    fn a_proxy_does_not_change_oversized_handshake_classification() {
        let connection = ConnectionDetails {
            proxy: Some(PROXY.into()),
            ..connection()
        };

        assert_eq!(
            classify_tls_error(
                &oversized_handshake_error(),
                &connection,
                Stage::Handshaking
            ),
            Some(TlsFailure::OversizedHandshake)
        );
    }

    /// Starts from the configuration a device would be left in, rather than from an error assumed
    /// to arise from it: two certificates are created, and the key of one is offered with the
    /// certificate of the other
    #[test]
    fn reads_a_key_that_does_not_belong_to_its_certificate_as_an_unpaired_key() {
        let dir = tempfile::tempdir().unwrap();
        let (cert_path, _) = write_selfsigned_pair(dir.path(), "device-a");
        let (_, other_key_path) = write_selfsigned_pair(dir.path(), "device-b");

        let err = MqttAuthConfigCloudBroker {
            ca_path: cert_path.clone(),
            client: MqttAuthClientConfigCloudBroker {
                cert_file: cert_path,
                private_key: PrivateKeyType::File(other_key_path),
            },
        }
        .to_rustls_client_config()
        .expect_err("a certificate and a key that do not belong together cannot be used");

        assert_eq!(
            classify_config_error(&err),
            Some(TlsFailure::UnpairedPrivateKey)
        );
    }

    /// A configuration that fails for some other reason is left to report itself
    #[test]
    fn leaves_an_unrecognised_configuration_error_unclassified() {
        let dir = tempfile::tempdir().unwrap();
        let (cert_path, key_path) = write_selfsigned_pair(dir.path(), "device-a");
        std::fs::write(&cert_path, "not a certificate").unwrap();

        let err = MqttAuthConfigCloudBroker {
            ca_path: cert_path.clone(),
            client: MqttAuthClientConfigCloudBroker {
                cert_file: cert_path,
                private_key: PrivateKeyType::File(key_path),
            },
        }
        .to_rustls_client_config()
        .expect_err("a certificate file that holds no certificate cannot be used");

        assert_eq!(classify_config_error(&err), None);
    }

    #[test]
    fn reads_an_unknown_issuer_as_a_server_the_device_does_not_trust() {
        let err = std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            Error::InvalidCertificate(CertificateError::UnknownIssuer),
        );

        assert_eq!(
            classify_tls_error(&err, &connection(), Stage::Handshaking),
            Some(TlsFailure::UntrustedServer)
        );
    }

    #[test]
    fn leaves_an_unrecognised_error_unclassified() {
        let err = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "connection refused");

        assert_eq!(
            classify_tls_error(&err, &connection(), Stage::Handshaking),
            None
        );
    }

    #[test]
    fn explains_a_valid_but_rejected_certificate_exactly() {
        assert_eq!(
            TlsFailure::DeviceCertificateRejected.explain(
                &connection(),
                Some(ValidityStatus::Valid {
                    expired_in: Duration::from_secs(86400),
                }),
            ),
            "Cumulocity did not recognise the device certificate.\n\n\
             The certificate sent was /etc/tedge/device-certs/tedge-certificate.pem (set by \
             'c8y.device.cert_path'), which identifies this device as 'test-device'.\n\n\
             Check that this is the certificate registered as trusted in the tenant at \
             example.cumulocity.com. If it is not, upload it with `tedge cert upload c8y`."
        );
    }

    #[test]
    fn explains_an_unreadable_rejected_certificate_exactly() {
        assert_eq!(
            TlsFailure::DeviceCertificateRejected.explain(&connection(), None),
            "Cumulocity did not recognise the device certificate.\n\n\
             The certificate sent was /etc/tedge/device-certs/tedge-certificate.pem (set by \
             'c8y.device.cert_path'), which identifies this device as 'test-device'.\n\n\
             Check that this is the certificate registered as trusted in the tenant at \
             example.cumulocity.com. If it is not, upload it with `tedge cert upload c8y`."
        );
    }

    #[test]
    fn explains_an_expired_rejected_certificate_exactly() {
        assert_eq!(
            TlsFailure::DeviceCertificateRejected.explain(
                &connection(),
                Some(ValidityStatus::Expired {
                    since: Duration::from_secs(3 * 86400),
                }),
            ),
            "The device certificate expired 3 days ago, so example.cumulocity.com rejected it.\n\n\
             The certificate sent was /etc/tedge/device-certs/tedge-certificate.pem (set by \
             'c8y.device.cert_path'), which identifies this device as 'test-device'.\n\n\
             Renew the certificate with `tedge cert renew --ca self-signed`, then upload the \
             renewed certificate with `tedge cert upload c8y`."
        );
    }

    #[test]
    fn explains_a_future_rejected_certificate_exactly() {
        assert_eq!(
            TlsFailure::DeviceCertificateRejected.explain(
                &connection(),
                Some(ValidityStatus::NotValidYet {
                    valid_in: Duration::from_secs(2 * 86400),
                }),
            ),
            "The device certificate is not valid until 2 days from now, so \
             example.cumulocity.com rejected it.\n\n\
             The certificate sent was /etc/tedge/device-certs/tedge-certificate.pem (set by \
             'c8y.device.cert_path'), which identifies this device as 'test-device'.\n\n\
             Check the clock on this device and the service that keeps it in sync. If the clock \
             is correct, renew the certificate with `tedge cert renew --ca self-signed` and \
             upload it with `tedge cert upload c8y`."
        );
    }

    #[test]
    fn names_both_files_that_have_to_match_when_the_key_does_not_match_the_certificate() {
        let explanation = TlsFailure::UnpairedPrivateKey.explain(&connection(), None);

        assert!(
            explanation.contains(CERT_PATH) && explanation.contains(KEY_PATH),
            "should name both files sent: {explanation}"
        );
        assert!(
            explanation.contains("'c8y.device.cert_path'")
                && explanation.contains("'c8y.device.key_path'"),
            "should name the settings that chose them: {explanation}"
        );
    }

    #[test]
    fn names_the_profiled_settings_when_the_connection_uses_a_profile() {
        let connection = ConnectionDetails {
            config_prefix: "c8y.profiles.staging".into(),
            ..connection()
        };

        let explanation = TlsFailure::UnpairedPrivateKey.explain(&connection, None);

        assert!(
            explanation.contains("'c8y.profiles.staging.device.key_path'"),
            "should name the profile's own setting: {explanation}"
        );
    }

    #[test]
    fn explains_a_generic_handshake_rejection_exactly() {
        assert_eq!(
            TlsFailure::HandshakeRejected.explain(&connection(), None),
            "example.cumulocity.com refused the TLS handshake without giving a reason.\n\n\
             Check that `example.cumulocity.com` is the correct MQTT endpoint for this tenant. If \
             it is, ask the Cumulocity administrator to check the tenant's TLS configuration and \
             server logs."
        );
    }

    #[test]
    fn adds_simple_proxy_triage_to_a_handshake_rejection() {
        let connection = ConnectionDetails {
            proxy: Some(PROXY.into()),
            ..connection()
        };

        assert_eq!(
            TlsFailure::HandshakeRejected.explain(&connection, None),
            "example.cumulocity.com refused the TLS handshake without giving a reason.\n\n\
             Check that `example.cumulocity.com` is the correct MQTT endpoint for this tenant. If \
             it is, ask the Cumulocity administrator to check the tenant's TLS configuration and \
             server logs.\n\n\
             This connection goes through proxy `proxy.example.com:3128`, which may also be \
             involved. Include its address when asking your network administrator or Cumulocity \
             administrator for help."
        );
    }

    #[test]
    fn gives_hsm_advice_with_profiled_keys_without_disclosing_the_uri() {
        let connection = ConnectionDetails {
            config_prefix: "c8y.profiles.staging".into(),
            client: ClientAuth::Certificate(ClientCredentials {
                cert_path: CERT_PATH.into(),
                key: KeyLocation::Token,
            }),
            ..connection()
        };

        assert_eq!(
            TlsFailure::HandshakeRejected.explain(&connection, None),
            "example.cumulocity.com refused the TLS handshake without giving a reason.\n\n\
             Check that `example.cumulocity.com` is the correct MQTT endpoint for this tenant. If \
             it is, ask the Cumulocity administrator to check the tenant's TLS configuration and \
             server logs.\n\n\
             This connection uses an HSM private key. Check that \
             `c8y.profiles.staging.device.cert_path` points to the certificate issued for the key \
             selected by `c8y.profiles.staging.device.key_uri`. The certificate supplied was \
             /etc/tedge/device-certs/tedge-certificate.pem."
        );
    }

    #[test]
    fn explains_a_direct_oversized_handshake_exactly() {
        assert_eq!(
            TlsFailure::OversizedHandshake.explain(&connection(), None),
            "Cumulocity sent a TLS handshake larger than the 64 KB thin-edge.io can accept.\n\n\
             The likely cause is an unnecessarily large `certificate_authorities` list. This \
             must be corrected on the Cumulocity side; changing this device's certificate or \
             configuration will not fix it.\n\n\
             Contact Cumulocity support and quote the MQTT host `example.cumulocity.com`."
        );
    }

    #[test]
    fn adds_simple_proxy_triage_to_an_oversized_handshake() {
        let connection = ConnectionDetails {
            proxy: Some(PROXY.into()),
            ..connection()
        };

        assert_eq!(
            TlsFailure::OversizedHandshake.explain(&connection, None),
            "Cumulocity sent a TLS handshake larger than the 64 KB thin-edge.io can accept.\n\n\
             The likely cause is an unnecessarily large `certificate_authorities` list. This \
             must be corrected on the Cumulocity side; changing this device's certificate or \
             configuration will not fix it.\n\n\
             Contact Cumulocity support and quote the MQTT host `example.cumulocity.com`.\n\n\
             This connection goes through proxy `proxy.example.com:3128`, which may be responsible \
             for the oversized handshake. Ask whoever manages that proxy to check it; if it is \
             only forwarding the connection, contact Cumulocity support and quote both \
             `example.cumulocity.com` and `proxy.example.com:3128`."
        );
    }

    #[test]
    fn points_at_the_root_certificates_when_the_device_does_not_trust_cumulocity() {
        let explanation = TlsFailure::UntrustedServer.explain(&connection(), None);

        assert!(
            explanation.contains(ROOT_CERT_PATH) && explanation.contains("'c8y.root_cert_path'"),
            "should name the authorities in use and the setting that chose them: {explanation}"
        );
        assert!(
            explanation.contains("tedge cert create"),
            "should say the device's own certificate is not at fault: {explanation}"
        );
    }

    #[test]
    fn error_context_says_whether_the_connection_was_ever_established() {
        assert_ne!(
            error_context(Stage::Connected),
            error_context(Stage::Handshaking)
        );
    }

    /// The certificate presented is the one the connection is configured with, which is not always
    /// the one named by `device.cert_path`: reconnecting to validate a renewed certificate sends
    /// the new one instead
    #[test]
    fn reads_the_certificate_from_the_authentication_config() {
        let mqtt_auth_config = MqttAuthConfigCloudBroker {
            ca_path: ROOT_CERT_PATH.into(),
            client: MqttAuthClientConfigCloudBroker {
                cert_file: format!("{CERT_PATH}.new").into(),
                private_key: PrivateKeyType::File(KEY_PATH.into()),
            },
        };

        let connection = ConnectionDetails::new(
            HOST,
            DEVICE_ID,
            None,
            &mqtt_auth_config,
            AuthType::Certificate,
            None,
        );

        let explanation = TlsFailure::DeviceCertificateRejected.explain(&connection, None);

        assert!(
            explanation.contains(&format!("{CERT_PATH}.new")),
            "should name the certificate actually sent: {explanation}"
        );
    }

    #[test]
    fn qualifies_the_settings_with_the_profile_the_connection_uses() {
        let profile: ProfileName = "staging".parse().unwrap();

        let connection = ConnectionDetails::new(
            HOST,
            DEVICE_ID,
            Some(&profile),
            &MqttAuthConfigCloudBroker {
                ca_path: ROOT_CERT_PATH.into(),
                client: MqttAuthClientConfigCloudBroker {
                    cert_file: CERT_PATH.into(),
                    private_key: PrivateKeyType::File(KEY_PATH.into()),
                },
            },
            AuthType::Certificate,
            None,
        );

        assert_eq!(
            connection.config_key("device.key_path"),
            "c8y.profiles.staging.device.key_path"
        );
    }

    const HOST: &str = "example.cumulocity.com";
    const DEVICE_ID: &str = "test-device";
    const CERT_PATH: &str = "/etc/tedge/device-certs/tedge-certificate.pem";
    const KEY_PATH: &str = "/etc/tedge/device-certs/tedge-private-key.pem";
    const ROOT_CERT_PATH: &str = "/etc/ssl/certs";
    const PROXY: &str = "proxy.example.com:3128";

    /// A connection authenticated with a certificate and key held in files, as `tedge cert create`
    /// leaves them
    fn connection() -> ConnectionDetails {
        ConnectionDetails {
            host: HOST.into(),
            device_id: DEVICE_ID.into(),
            config_prefix: "c8y".into(),
            root_cert_path: ROOT_CERT_PATH.into(),
            client: ClientAuth::Certificate(ClientCredentials {
                cert_path: CERT_PATH.into(),
                key: KeyLocation::File(KEY_PATH.into()),
            }),
            proxy: None,
        }
    }

    /// The largest handshake flight `rustls` will buffer, as a guard against denial-of-service
    ///
    /// Mirrors `MAX_HANDSHAKE_SIZE` in `rustls`, which is not publicly exported
    const MAX_HANDSHAKE_SIZE: usize = 0xffff;

    /// Writes a fresh self-signed certificate and its private key, returning their paths
    ///
    /// Each call produces its own key, so a certificate from one call and a key from another are
    /// exactly the unpaired configuration a device can end up in
    fn write_selfsigned_pair(dir: &std::path::Path, id: &str) -> (Utf8PathBuf, Utf8PathBuf) {
        write_selfsigned_pair_valid_for(
            dir,
            id,
            certificate::CsrTemplate::default().validity_period_days,
        )
    }

    /// Writes a certificate that stops being valid after the given number of days
    ///
    /// Certificates are dated from yesterday, so a lifetime of no days at all is one that has run
    /// out — the state a device is left in when nothing renewed its certificate in time
    fn write_selfsigned_pair_valid_for(
        dir: &std::path::Path,
        id: &str,
        days: u32,
    ) -> (Utf8PathBuf, Utf8PathBuf) {
        let pair = certificate::KeyCertPair::new_selfsigned_certificate(
            &certificate::CsrTemplate {
                validity_period_days: days,
                ..Default::default()
            },
            id,
            &certificate::KeyKind::New,
        )
        .expect("a self-signed certificate can be created");

        let cert_path = Utf8PathBuf::try_from(dir.join(format!("{id}.crt"))).unwrap();
        let key_path = Utf8PathBuf::try_from(dir.join(format!("{id}.key"))).unwrap();
        std::fs::write(&cert_path, pair.certificate_pem_string().unwrap()).unwrap();
        std::fs::write(&key_path, pair.private_key_pem_string().unwrap().as_str()).unwrap();
        (cert_path, key_path)
    }

    /// Writes a certificate that only becomes valid in the future, as a device whose clock is set
    /// behind the real date sees its own
    ///
    /// Built with `rcgen` directly: `tedge` has no way to issue one, which is the point — nothing
    /// short of a wrong clock produces it
    fn write_certificate_valid_from(dir: &std::path::Path, id: &str, days: i64) -> Utf8PathBuf {
        let key = rcgen::KeyPair::generate().expect("a key pair can be generated");
        let mut params =
            rcgen::CertificateParams::new(vec![id.to_owned()]).expect("valid certificate params");
        params.not_before = time::OffsetDateTime::now_utc() + time::Duration::days(days);
        params.not_after = params.not_before + time::Duration::days(1);
        let certificate = params.self_signed(&key).expect("a self-signed certificate");

        let cert_path = Utf8PathBuf::try_from(dir.join(format!("{id}-later.crt"))).unwrap();
        std::fs::write(&cert_path, certificate.pem()).unwrap();
        cert_path
    }

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
