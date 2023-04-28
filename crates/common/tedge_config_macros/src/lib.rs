#[doc(inline)]
pub use tedge_config_macros_macro::define_tedge_config;

extern crate self as tedge_config_macros;

use camino::Utf8PathBuf;
pub use connect_url::*;
use default::*;
use doku_aliases::*;
pub use option::*;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::num::NonZeroU16;
use std::path::PathBuf;

mod connect_url;
mod default;
mod doku_aliases;
mod option;

static DEFAULT_ROOT_CERT_PATH: &str = "/etc/ssl/certs";

define_tedge_config! {
    device: {
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
        #[serde(rename = "type")]
        device_type: String,
    },

    #[serde(alias = "azure")] // for 0.1.0 compatibility
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

    mqtt: {
        bind: {
            /// The address mosquitto binds to for internal use
            #[tedge_config(example = "127.0.0.1", default(variable = "Ipv4Addr::LOCALHOST"))]
            address: IpAddr,

            /// The port mosquitto binds to for internal use
            #[tedge_config(example = "1883", default(function = "default_mqtt_port"))]
            #[doku(as = "u16")]
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
            #[tedge_config(default(from_path = "mqtt.bind.port"))]
            #[doku(as = "u16")]
            port: NonZeroU16,

            auth: {
                /// Path to the CA certificate used by MQTT clients to use when authenticating the MQTT broker
                #[tedge_config(example = "/etc/mosquitto/ca_certificates/ca.crt")]
                #[doku(as = "PathBuf")]
                #[serde(alias = "cafile")]
                ca_file: Utf8PathBuf,

                /// Path to the directory containing the CA certificates used by MQTT
                /// clients when authenticating the MQTT broker
                #[tedge_config(example = "/etc/mosquitto/ca_certificates")]
                #[doku(as = "PathBuf")]
                #[serde(alias = "capath")]
                ca_path: Utf8PathBuf,

                /// Path to the client certficate
                #[doku(as = "PathBuf")]
                #[serde(alias = "certfile")]
                cert_file: Utf8PathBuf,

                /// Path to the client private key
                #[doku(as = "PathBuf")]
                #[serde(alias = "keyfile")]
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
            #[serde(alias = "capath")]
            ca_path: Utf8PathBuf,

            /// Path to the certificate file which is used by the external MQTT listener
            #[tedge_config(note = "This setting shall be used together with `mqtt.external.key_file` for external connections.")]
            #[tedge_config(example = "/etc/tedge/device-certs/tedge-certificate.pem")]
            #[doku(as = "PathBuf")]
            #[serde(alias = "certfile")]
            cert_file: Utf8PathBuf,

            /// Path to the key file which is used by the external MQTT listener
            #[tedge_config(note = "This setting shall be used together with `mqtt.external.cert_file` for external connections.")]
            #[tedge_config(example = "/etc/tedge/device-certs/tedge-private-key.pem")]
            #[doku(as = "PathBuf")]
            #[serde(alias = "keyfile")]
            key_file: Utf8PathBuf,
        }
    }
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

#[test]
fn root_cert_path_default() {
    let dto = TEdgeConfigDto::default();
    let reader = TEdgeConfigReader::from_dto(&dto, &TEdgeConfigLocation);
    assert_eq!(reader.az.root_cert_path, "/etc/ssl/certs");
}

#[test]
fn writable_keys_can_be_parsed_from_aliases() {
    let _: WritableKey = "az.mapper.timestamp".parse().unwrap();
    let _: WritableKey = "azure.mapper.timestamp".parse().unwrap();
}

#[test]
fn readable_keys_can_be_parsed_from_aliases() {
    let _: ReadableKey = "az.mapper.timestamp".parse().unwrap();
    let _: ReadableKey = "azure.mapper.timestamp".parse().unwrap();
}

#[test]
fn default_from_path_uses_the_correct_default() {
    #![allow(unused_variables)]
    define_tedge_config! {
        test: {
            #[tedge_config(default(value = "DEFAULT_VALUE_FOR_ONE"))]
            one: String,
            #[tedge_config(default(from_path = "test.one"))]
            two: String,
        }
    }
    let dto = TEdgeConfigDto::default();
    assert_eq!(
        TEdgeConfigReader::from_dto(&dto, &TEdgeConfigLocation)
            .test
            .two,
        "DEFAULT_VALUE_FOR_ONE"
    );
}

#[test]
fn default_from_path_uses_the_value_of_other_field_if_set() {
    #![allow(unused_variables)]
    define_tedge_config! {
        test: {
            #[tedge_config(default(value = "DEFAULT_VALUE_FOR_ONE"))]
            one: String,
            #[tedge_config(default(from_path = "test.one"))]
            two: String,
        }
    }
    let mut dto = TEdgeConfigDto::default();
    dto.test.one = Some("UPDATED_VALUE".into());
    assert_eq!(
        TEdgeConfigReader::from_dto(&dto, &TEdgeConfigLocation)
            .test
            .two,
        "UPDATED_VALUE"
    );
}

#[test]
fn default_from_path_uses_its_own_value_if_both_are_set() {
    #![allow(unused_variables)]
    define_tedge_config! {
        test: {
            #[tedge_config(default(value = "DEFAULT_VALUE_FOR_ONE"))]
            one: String,
            #[tedge_config(default(from_path = "test.one"))]
            two: String,
        }
    }
    let mut dto = TEdgeConfigDto::default();
    dto.test.one = Some("UPDATED_VALUE".into());
    dto.test.two = Some("OWN_VALUE".into());
    assert_eq!(
        TEdgeConfigReader::from_dto(&dto, &TEdgeConfigLocation)
            .test
            .two,
        "OWN_VALUE"
    );
}
