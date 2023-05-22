use camino::Utf8PathBuf;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::num::NonZeroU16;
use std::path::PathBuf;
use tedge_config_macros::*;

static DEFAULT_ROOT_CERT_PATH: &str = "/etc/ssl/certs";

#[derive(thiserror::Error, Debug)]
pub enum ReadError {
    #[error(transparent)]
    ConfigNotSet(#[from] ConfigNotSet),
    #[error("Something went wrong: {0}")]
    GenericError(String),
}

define_tedge_config! {
    #[tedge_config(reader(skip))]
    config: {
        #[tedge_config(default(value = 1))]
        version: u32,
    },

    device: {
        #[tedge_config(readonly(
            write_error = "\
                The device id is read from the device certificate and cannot be set directly.\n\
                To set 'device.id' to some <id>, you can use `tedge cert create --device-id <id>`.",
            function = "device_id",
        ))]
        #[doku(as = "String")]
        id: Result<String, ReadError>,

        /// Path where the device's private key is stored
        #[tedge_config(example = "/etc/tedge/device-certs/tedge-private-key.pem", default(function = "default_device_key"))]
        #[doku(as = "PathBuf")]
        key_path: Utf8PathBuf,

        /// Path where the device's certificate is stored
        #[tedge_config(example = "/etc/tedge/device-certs/tedge-certificate.pem", default(function = "default_device_cert"))]
        #[doku(as = "PathBuf")]
        cert_path: Utf8PathBuf,

        /// The default device type
        #[tedge_config(example = "thin-edge.io")]
        #[tedge_config(rename = "type")]
        device_type: String,
    },

    #[tedge_config(deprecated_name = "azure")] // for 0.1.0 compatibility
    az: {
        /// Endpoint URL of Azure IoT tenant
        #[tedge_config(example = "myazure.azure-devices.net")]
        url: ConnectUrl,

        /// The path where Azure IoT root certificate(s) are stared
        #[tedge_config(note = "The value can be a directory path as well as the path of the direct certificate file.")]
        #[tedge_config(example = "/etc/tedge/az-trusted-root-certificates.pem", default(variable = "DEFAULT_ROOT_CERT_PATH"))]
        #[doku(as = "PathBuf")]
        root_cert_path: Utf8PathBuf,

        mapper: {
            /// Whether the Azure IoT mapper should add a timestamp or not
            #[tedge_config(example = "true")]
            #[tedge_config(default(value = true))]
            timestamp: bool,
        }
    },

    c8y: {
        #[tedge_config(reader(private))]
        url: String,

        http: {
            #[tedge_config(default(from_optional_key = "c8y.url"))]
            url: String,
        }
    },

    mqtt: {
        bind: {
            /// The address mosquitto binds to for internal use
            #[tedge_config(example = "127.0.0.1", default(variable = "Ipv4Addr::LOCALHOST"))]
            address: IpAddr,

            /// The port mosquitto binds to for internal use
            #[tedge_config(example = "1883", default(function = "default_mqtt_port"))]
            #[doku(as = "u16")]
            #[tedge_config(deprecated_key = "mqtt.bind.port")]
            // This was originally u16, but I can't think of any way in which
            // tedge could actually connect to mosquitto if it bound to a random
            // free port, so I don't think 0 is *really* valid here
            port: NonZeroU16,
        },

        client: {
            /// The host that the thin-edge MQTT client should connect to
            #[tedge_config(example = "localhost", default(value = "localhost"))]
            host: String,

            /// The port that the thin-edge MQTT client should connect to
            #[tedge_config(default(from_key = "mqtt.bind.port"))]
            #[doku(as = "u16")]
            port: NonZeroU16,

            auth: {
                /// Path to the CA certificate used by MQTT clients to use when authenticating the MQTT broker
                #[tedge_config(example = "/etc/mosquitto/ca_certificates/ca.crt")]
                #[doku(as = "PathBuf")]
                #[tedge_config(deprecated_name = "cafile")]
                ca_file: Utf8PathBuf,

                /// Path to the directory containing the CA certificates used by MQTT
                /// clients when authenticating the MQTT broker
                #[tedge_config(example = "/etc/mosquitto/ca_certificates")]
                #[doku(as = "PathBuf")]
                #[tedge_config(deprecated_name = "capath")]
                ca_path: Utf8PathBuf,

                /// Path to the client certificate
                #[doku(as = "PathBuf")]
                #[tedge_config(deprecated_name = "certfile")]
                cert_file: Utf8PathBuf,

                /// Path to the client private key
                #[doku(as = "PathBuf")]
                #[tedge_config(deprecated_name = "keyfile")]
                key_file: Utf8PathBuf,
            }
        },

        external: {
            bind: {
                /// The port mosquitto binds to for external use
                #[tedge_config(example = "8883")]
                port: u16,

                /// The address mosquitto binds to for external use
                #[tedge_config(example = "0.0.0.0")]
                address: IpAddr,

                /// Name of the network interface which mosquitto limits incoming connections on
                #[tedge_config(example = "wlan0")]
                interface: String,
            },

            /// Path to a file containing the PEM encoded CA certificates that are
            /// trusted when checking incoming client certificates
            #[tedge_config(example = "/etc/ssl/certs")]
            #[doku(as = "PathBuf")]
            #[tedge_config(deprecated_name = "capath")]
            ca_path: Utf8PathBuf,

            /// Path to the certificate file which is used by the external MQTT listener
            #[tedge_config(note = "This setting shall be used together with `mqtt.external.key_file` for external connections.")]
            #[tedge_config(example = "/etc/tedge/device-certs/tedge-certificate.pem")]
            #[doku(as = "PathBuf")]
            #[tedge_config(deprecated_name = "certfile")]
            cert_file: Utf8PathBuf,

            /// Path to the key file which is used by the external MQTT listener
            #[tedge_config(note = "This setting shall be used together with `mqtt.external.cert_file` for external connections.")]
            #[tedge_config(example = "/etc/tedge/device-certs/tedge-private-key.pem")]
            #[doku(as = "PathBuf")]
            #[tedge_config(deprecated_name = "keyfile")]
            key_file: Utf8PathBuf,
        }
    }
}

fn device_id(_reader: &TEdgeConfigReader) -> Result<String, ReadError> {
    Ok("dummy-device-id".to_owned())
}

fn default_device_key(location: &TEdgeConfigLocation) -> Utf8PathBuf {
    location
        .tedge_config_root_path()
        .join("device-certs")
        .join("tedge-private-key.pem")
}

fn default_device_cert(location: &TEdgeConfigLocation) -> Utf8PathBuf {
    location
        .tedge_config_root_path()
        .join("device-certs")
        .join("tedge-certificate.pem")
}

fn default_mqtt_port() -> NonZeroU16 {
    NonZeroU16::try_from(1883).unwrap()
}

fn main() {
    let mut dto = TEdgeConfigDto::default();
    dto.mqtt.bind.address = Some(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)));

    let config = TEdgeConfigReader::from_dto(&dto, &TEdgeConfigLocation);

    // Typed reads
    println!(
        "Device id is {}.",
        // We have to pass the config into try_read to avoid TEdgeConfigReader being
        // self-referential
        config.device.id.try_read(&config).as_ref().unwrap()
    );
    assert_eq!(u16::from(config.mqtt.bind.port), 1883);
    assert_eq!(config.mqtt.external.bind.port.or_none(), None);
    assert_eq!(
        config.read_string(ReadableKey::DeviceId).unwrap(),
        "dummy-device-id"
    );
}
