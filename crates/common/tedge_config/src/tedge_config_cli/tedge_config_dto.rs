//! Crate-private plain-old data-type used for serialization.

use std::num::NonZeroU16;
use std::path::PathBuf;

use crate::*;
use camino::Utf8PathBuf;
use doku::Document;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Default, Deserialize, Serialize, Document)]
pub struct TEdgeConfigDto {
    /// Captures the device specific configurations
    #[serde(default)]
    pub(crate) device: DeviceConfigDto,

    /// Captures the configurations required to connect to Cumulocity
    #[serde(default)]
    pub(crate) c8y: CumulocityConfigDto,

    #[serde(default, alias = "azure")] // for version 0.1.0 compatibility
    pub(crate) az: AzureConfigDto,

    #[serde(default)]
    pub(crate) aws: AwsConfigDto,

    #[serde(default)]
    pub(crate) mqtt: MqttConfigDto,

    #[serde(default)]
    pub(crate) http: HttpConfigDto,

    #[serde(default)]
    pub(crate) software: SoftwareConfigDto,

    #[serde(default)]
    pub(crate) tmp: PathConfigDto,

    #[serde(default)]
    pub(crate) logs: PathConfigDto,

    #[serde(default)]
    pub(crate) run: PathConfigDto,

    #[serde(default)]
    pub(crate) data: PathConfigDto,

    #[serde(default)]
    pub(crate) firmware: FirmwareConfigDto,

    #[serde(default)]
    pub(crate) service: ServiceTypeConfigDto,
}

/// Represents the device specific configurations defined in the [device]
/// section of the thin edge configuration TOML file
#[derive(Debug, Default, Deserialize, Serialize, Document)]
pub struct DeviceConfigDto {
    /// Path where the device's private key is stored
    #[doku(example = "/etc/tedge/device-certs/tedge-private-key.pem")]
    #[doku(as = "PathBuf")]
    pub(crate) key_path: Option<Utf8PathBuf>,

    /// Path where the device's certificate is stored
    #[doku(example = "/etc/tedge/device-certs/tedge-certificate.pem")]
    #[doku(as = "PathBuf")]
    pub(crate) cert_path: Option<Utf8PathBuf>,

    /// The default device type
    #[serde(rename = "type")]
    #[doku(example = "thin-edge.io")]
    pub(crate) device_type: Option<String>,
}

/// Represents the Cumulocity specific configurations defined in the
/// [c8y] section of the thin edge configuration TOML file
#[derive(Debug, Default, Deserialize, Serialize, Document)]
pub(crate) struct CumulocityConfigDto {
    /// Endpoint URL of the Cumulocity tenant
    #[doku(example = "your-tenant.cumulocity.com", as = "String")]
    pub(crate) url: Option<ConnectUrl>,

    /// The path where Cumulocity root certificate(s) are stored. The value can
    /// be a directory path as well as the path of the direct certificate file.
    #[doku(example = "/etc/tedge/c8y-trusted-root-certificates.pem")]
    #[doku(as = "PathBuf")]
    pub(crate) root_cert_path: Option<Utf8PathBuf>,

    /// Set of c8y template IDs used for subscriptions
    #[doku(literal_example = "templateId1,templateId2", as = "String")]
    pub(crate) smartrest_templates: Option<TemplatesSet>,
}

#[derive(Debug, Default, Deserialize, Serialize, Document)]
pub(crate) struct AzureConfigDto {
    /// Endpoint URL of Azure IoT tenant
    #[doku(example = "myazure.azure-devices.net", as = "String")]
    pub(crate) url: Option<ConnectUrl>,

    /// The path where Azure root certificate(s) are stored
    #[doku(example = "/etc/tedge/az-trusted-root-certificates.pem")]
    #[doku(as = "PathBuf")]
    pub(crate) root_cert_path: Option<Utf8PathBuf>,

    /// Whether Azure mapper should add timestamp or not
    #[doku(example = "true")]
    pub(crate) mapper_timestamp: Option<bool>,
}

#[derive(Debug, Default, Deserialize, Serialize, Document)]
pub(crate) struct AwsConfigDto {
    /// Endpoint URL of AWS instance
    #[doku(example = "your-endpoint.amazonaws.com", as = "String")]
    pub(crate) url: Option<ConnectUrl>,

    /// The path where AWS root certificate(s) are stored
    #[doku(example = "/etc/tedge/aws-trusted-root-certificates.pem")]
    #[doku(as = "PathBuf")]
    pub(crate) root_cert_path: Option<Utf8PathBuf>,

    /// Whether Azure mapper should add timestamp or not
    #[doku(example = "true")]
    pub(crate) mapper_timestamp: Option<bool>,
}

#[derive(Debug, Default, Deserialize, Serialize, Document)]
pub(crate) struct MqttConfigDto {
    /// The address mosquitto binds to for internal use
    pub(crate) bind_address: Option<IpAddress>,
    // TODO example
    /// The port mosquitto binds to for internal use
    pub(crate) port: Option<u16>,

    pub(crate) client_host: Option<String>,

    /// Mqtt broker port, which is used by the mqtt clients to publish or
    /// subscribe
    #[doku(example = "1883", as = "u16")]
    // When connecting to a host, port 0 is invalid. When binding, however, port
    // 0 is accepted and understood by the system to dynamically assign any free
    // port to the process. The process then needs to take notice of what port
    // it received, which I'm not sure if we're doing.
    //
    // If we don't want to allow binding to port 0, then we can also use
    // `NonZeroU16` there as well, which because it can never be 0, can make the
    // `Option` completely free, because Option can use 0x0000 value for the
    // `None` variant.
    pub(crate) client_port: Option<NonZeroU16>,

    /// Path to the trusted CA certificate file used by MQTT clients when
    /// authenticating the MQTT broker.
    #[doku(example = "/etc/mosquitto/ca_certificates/ca.crt", as = "PathBuf")]
    pub(crate) client_ca_file: Option<Utf8PathBuf>,

    /// Path to the directory containing trusted CA certificates used by MQTT
    /// clients when authenticating the MQTT broker.
    #[doku(example = "/etc/mosquitto/ca_certificates", as = "PathBuf")]
    pub(crate) client_ca_path: Option<Utf8PathBuf>,

    /// MQTT client authentication configuration, containing a path to a client
    /// certificate and a private key.
    pub(crate) client_auth: Option<MqttClientAuthConfig>,

    /// The port mosquitto binds to for external use
    #[doku(example = "1883")]
    pub(crate) external_port: Option<u16>,

    /// The address mosquitto binds to for external use
    #[doku(example = "0.0.0.0")]
    pub(crate) external_bind_address: Option<IpAddress>,

    /// The interface mosquitto listens on for external use
    #[doku(example = "wlan0")]
    pub(crate) external_bind_interface: Option<String>,

    // All the paths relating to mosquitto are strings as they need to be safe
    // to write to a configuration file (i.e. probably valid utf-8 at the least)
    /// Path to a file containing the PEM encoded CA certificates that are
    /// trusted when checking incoming client certificates
    #[doku(example = "/etc/ssl/certs", as = "PathBuf")]
    pub(crate) external_capath: Option<Utf8PathBuf>,

    /// Path to the certificate file which is used by the external MQTT listener
    #[doku(
        example = "/etc/tedge/device-certs/tedge-certificate.pem",
        as = "PathBuf"
    )]
    pub(crate) external_certfile: Option<Utf8PathBuf>,

    /// Path to the key file which is used by the external MQTT listener
    #[doku(example = "/etc/tedge/device-certs/tedge-private-key.pem")]
    #[doku(as = "PathBuf")]
    pub(crate) external_keyfile: Option<Utf8PathBuf>,
}

#[derive(Debug, Default, Deserialize, Serialize, Document)]
pub(crate) struct HttpConfigDto {
    /// HTTP server port used by the File Transfer Service
    #[doku(example = "8000")]
    #[serde(alias = "bind_port")]
    pub(crate) port: Option<u16>,

    /// HTTP bind address used by the File Transfer service
    #[doku(example = "127.0.0.1")]
    #[doku(example = "192.168.1.2")]
    pub(crate) bind_address: Option<IpAddress>,
}

#[derive(Debug, Default, Deserialize, Serialize, Document)]
pub struct SoftwareConfigDto {
    pub(crate) default_plugin_type: Option<String>,
}

#[derive(Debug, Default, Deserialize, Serialize, Document)]
pub struct PathConfigDto {
    #[serde(rename = "path")]
    #[doku(as = "PathBuf")]
    pub(crate) dir_path: Option<Utf8PathBuf>,

    /// Whether create lock file or not
    pub(crate) lock_files: Option<bool>,
}

#[derive(Debug, Default, Deserialize, Serialize, Document)]
pub struct FirmwareConfigDto {
    pub(crate) child_update_timeout: Option<u64>,
}

#[derive(Debug, Default, Deserialize, Serialize, Document)]
pub struct ServiceTypeConfigDto {
    #[serde(rename = "type")]
    pub(crate) service_type: Option<String>,
}

/// Contains MQTT client authentication configuration.
///
// Despite both cert_file and key_file being required for client authentication,
// fields in this struct are optional because `tedge config set` needs to
// successfully parse the configuration, update it in memory, and then save
// deserialized object. If the upcoming configuration refactor discussed in [1]
// ends up supporting partial updates to such objects, then these fields could
// be made non-optional.
//
// [1]: https://github.com/thin-edge/thin-edge.io/issues/1812
#[derive(Debug, Default, Deserialize, Serialize, Document)]
pub(crate) struct MqttClientAuthConfig {
    /// Path to the client certificate
    #[doku(example = "/path/to/client.crt", as = "PathBuf")]
    pub cert_file: Option<Utf8PathBuf>,

    /// Path to the client private key
    #[doku(example = "/path/to/client.key", as = "PathBuf")]
    pub key_file: Option<Utf8PathBuf>,
}

#[cfg(test)]
mod tests {
    use core::panic;
    use std::borrow::Cow;

    use figment::providers::Format;
    use serde::de::DeserializeOwned;

    use super::*;

    #[test]
    fn example_values_can_be_deserialised() {
        let ty = TEdgeConfigDto::ty();
        let doku::TypeKind::Struct { fields, transparent: false } = ty.kind else { panic!("Expected struct but got {:?}", ty.kind) };
        let doku::Fields::Named { fields } = fields else { panic!("Expected named fields but got {:?}", fields)};
        for (key, ty) in struct_field_paths(None, &fields) {
            verify_examples_for::<TEdgeConfigDto>(&key, ty)
        }
    }

    fn verify_examples_for<Dto>(key: &str, ty: doku::Type)
    where
        Dto: Default + Serialize + DeserializeOwned,
    {
        for example in ty.example.iter().flat_map(|e| e.iter()) {
            println!("Testing {key}={example}");
            figment::Jail::expect_with(|jail| {
                jail.set_env(key, example);
                let figment = figment::Figment::new()
                    .merge(figment::providers::Toml::string(
                        &toml::to_string(&Dto::default()).unwrap(),
                    ))
                    .merge(figment::providers::Env::raw().split("."));

                figment.extract::<Dto>().unwrap_or_else(|_| {
                    panic!("\n\nFailed to deserialize example data: {key}={example}\n\n")
                });

                Ok(())
            });
        }
    }

    fn key_name(prefix: Option<&str>, name: &'static str) -> Cow<'static, str> {
        match prefix {
            Some(prefix) => Cow::Owned(format!("{prefix}.{name}")),
            None => Cow::Borrowed(name),
        }
    }

    fn struct_field_paths(
        prefix: Option<&str>,
        fields: &[(&'static str, doku::Field)],
    ) -> Vec<(Cow<'static, str>, doku::Type)> {
        fields
            .iter()
            .flat_map(|(name, field)| match named_fields(&field.ty.kind) {
                Some(fields) => struct_field_paths(Some(&key_name(prefix, name)), fields),
                None => vec![(key_name(prefix, name), field.ty.clone())],
            })
            .collect()
    }

    fn named_fields(kind: &doku::TypeKind) -> Option<&[(&'static str, doku::Field)]> {
        match kind {
            doku::TypeKind::Struct {
                fields: doku::Fields::Named { fields },
                transparent: false,
            } => Some(fields),
            _ => None,
        }
    }
}
