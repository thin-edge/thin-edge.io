//! Crate-private plain-old data-type used for serialization.

use std::borrow::Cow;
use std::num::NonZeroU16;
use std::path::PathBuf;
use std::time::Duration;

use crate::*;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use doku::Document;
use serde::Deserialize;
use serde::Serialize;

#[cfg(test)]
use fake::Fake;

#[derive(Debug, Default, Deserialize, Serialize, Document, PartialEq, Eq)]
#[cfg_attr(test, derive(fake::Dummy))]
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
    pub(crate) tmp: TmpPathConfigDto,

    #[serde(default)]
    pub(crate) logs: LogsPathConfigDto,

    #[serde(default)]
    pub(crate) run: LockPathConfigDto,

    #[serde(default)]
    pub(crate) data: DataPathConfigDto,

    #[serde(default)]
    pub(crate) firmware: FirmwareConfigDto,

    #[serde(default)]
    pub(crate) service: ServiceTypeConfigDto,
}

impl TEdgeConfigDto {
    pub(crate) fn device(&self) -> &DeviceConfigDto {
        &self.device
    }

    pub(crate) fn c8y(&self) -> &CumulocityConfigDto {
        &self.c8y
    }

    pub(crate) fn az(&self) -> &AzureConfigDto {
        &self.az
    }

    pub(crate) fn aws(&self) -> &AwsConfigDto {
        &self.aws
    }

    pub(crate) fn mqtt(&self) -> &MqttConfigDto {
        &self.mqtt
    }

    pub(crate) fn http(&self) -> &HttpConfigDto {
        &self.http
    }

    pub(crate) fn software(&self) -> &SoftwareConfigDto {
        &self.software
    }

    pub(crate) fn tmp(&self) -> &TmpPathConfigDto {
        &self.tmp
    }

    pub(crate) fn data(&self) -> &DataPathConfigDto {
        &self.data
    }

    pub(crate) fn run(&self) -> &LockPathConfigDto {
        &self.run
    }

    pub(crate) fn logs(&self) -> &LogsPathConfigDto {
        &self.logs
    }

    pub(crate) fn firmware(&self) -> &FirmwareConfigDto {
        &self.firmware
    }

    pub(crate) fn service(&self) -> &ServiceTypeConfigDto {
        &self.service
    }
}

/// Represents the device specific configurations defined in the \[device\] section
/// of the thin edge configuration TOML file
#[derive(Debug, Default, Deserialize, Serialize, Document, PartialEq, Eq)]
#[cfg_attr(test, derive(fake::Dummy))]
pub struct DeviceConfigDto {
    /// Path where the device's private key is stored
    #[doku(example = "/etc/tedge/device-certs/tedge-private-key.pem")]
    #[doku(as = "PathBuf")]
    pub(crate) key_path: Option<Fakeable<Utf8PathBuf>>,

    /// Path where the device's certificate is stored
    #[doku(example = "/etc/tedge/device-certs/tedge-certificate.pem")]
    #[doku(as = "PathBuf")]
    pub(crate) cert_path: Option<Fakeable<Utf8PathBuf>>,

    /// The default device type
    #[serde(rename = "type")]
    #[doku(example = "thin-edge.io")]
    pub(crate) device_type: Option<String>,
}

impl DeviceConfigDto {
    pub(crate) fn key_path(&self, config_location: &TEdgeConfigLocation) -> Cow<Utf8Path> {
        if let Some(path) = &self.key_path {
            Cow::Borrowed(path)
        } else {
            Cow::Owned(
                config_location
                    .tedge_config_root_path()
                    .join("device-certs")
                    .join("tedge-private-key.pem"),
            )
        }
    }

    pub(crate) fn cert_path<'a>(
        &'a self,
        config_location: &TEdgeConfigLocation,
    ) -> Cow<'a, Utf8Path> {
        if let Some(path) = &self.cert_path {
            Cow::Borrowed(path)
        } else {
            Cow::Owned(
                config_location
                    .tedge_config_root_path()
                    .join("device-certs")
                    .join("tedge-certificate.pem"),
            )
        }
    }

    pub(crate) fn device_type(&self) -> &str {
        self.device_type.as_deref().unwrap_or("thin-edge.io")
    }
}

/// Represents the Cumulocity specific configurations defined in the
/// [c8y] section of the thin edge configuration TOML file
#[derive(Debug, Default, Deserialize, Serialize, Document, PartialEq, Eq)]
#[cfg_attr(test, derive(fake::Dummy))]
pub(crate) struct CumulocityConfigDto {
    /// Endpoint URL of the Cumulocity tenant
    #[doku(example = "your-tenant.cumulocity.com", as = "String")]
    pub(crate) url: Option<ConnectUrl>,

    /// The path where Cumulocity root certificate(s) are stored.
    #[doku(meta(
        "note = The value can be a directory path as well as the path of the direct certificate file."
    ))]
    #[doku(example = "/etc/tedge/c8y-trusted-root-certificates.pem")]
    #[doku(as = "PathBuf")]
    pub(crate) root_cert_path: Option<Fakeable<Utf8PathBuf>>,

    /// Set of SmartREST template IDs the device should subscribe to
    #[doku(literal_example = "templateId1,templateId2", as = "String")]
    pub(crate) smartrest_templates: Option<TemplatesSet>,
}

impl CumulocityConfigDto {
    pub(crate) fn url(&self) -> Option<&ConnectUrl> {
        self.url.as_ref()
    }

    pub(crate) fn root_cert_path(&self) -> &Utf8Path {
        self.root_cert_path
            .as_deref()
            .unwrap_or(Utf8Path::new("/etc/ssl/certs"))
    }

    pub(crate) fn smartrest_templates(&self) -> TemplatesSet {
        self.smartrest_templates
            .as_ref()
            .map_or_else(<_>::default, <_>::to_owned)
    }
}

#[derive(Debug, Default, Deserialize, Serialize, Document, PartialEq, Eq)]
#[cfg_attr(test, derive(fake::Dummy))]
pub(crate) struct AzureConfigDto {
    /// Endpoint URL of Azure IoT tenant
    #[doku(example = "myazure.azure-devices.net", as = "String")]
    pub(crate) url: Option<ConnectUrl>,

    /// The path where Azure IoT root certificate(s) are stored
    #[doku(meta(
        "note = The value can be a directory path as well as the path of the direct certificate file."
    ))]
    #[doku(example = "/etc/tedge/az-trusted-root-certificates.pem")]
    #[doku(as = "PathBuf")]
    pub(crate) root_cert_path: Option<Fakeable<Utf8PathBuf>>,

    /// Whether Azure mapper should add timestamp or not
    #[doku(example = "true")]
    pub(crate) mapper_timestamp: Option<bool>,
}

impl AzureConfigDto {
    pub(crate) fn url(&self) -> Option<&ConnectUrl> {
        self.url.as_ref()
    }

    pub(crate) fn root_cert_path(&self) -> &Utf8Path {
        self.root_cert_path
            .as_deref()
            .unwrap_or(Utf8Path::new("/etc/ssl/certs"))
    }

    pub(crate) fn mapper_timestamp(&self) -> bool {
        self.mapper_timestamp.unwrap_or(true)
    }
}

#[derive(Debug, Default, Deserialize, Serialize, Document, PartialEq, Eq)]
#[cfg_attr(test, derive(fake::Dummy))]
pub(crate) struct AwsConfigDto {
    /// Endpoint URL of AWS instance
    #[doku(example = "your-endpoint.amazonaws.com", as = "String")]
    pub(crate) url: Option<ConnectUrl>,

    /// The path where AWS IoT root certificate(s) are stored
    #[doku(meta(
        "note = The value can be a directory path as well as the path of the direct certificate file."
    ))]
    #[doku(example = "/etc/tedge/aws-trusted-root-certificates.pem")]
    #[doku(as = "PathBuf")]
    pub(crate) root_cert_path: Option<Fakeable<Utf8PathBuf>>,

    /// Whether AWS mapper should add timestamp or not
    #[doku(example = "true")]
    pub(crate) mapper_timestamp: Option<bool>,
}

impl AwsConfigDto {
    pub(crate) fn url(&self) -> Option<&ConnectUrl> {
        self.url.as_ref()
    }

    pub(crate) fn root_cert_path(&self) -> &Utf8Path {
        self.root_cert_path
            .as_deref()
            .unwrap_or_else(|| Utf8Path::new("/etc/ssl/certs"))
    }

    pub(crate) fn mapper_timestamp(&self) -> bool {
        self.mapper_timestamp.unwrap_or(true)
    }
}

#[derive(Debug, Default, Deserialize, Serialize, Document, PartialEq, Eq)]
#[cfg_attr(test, derive(fake::Dummy))]
pub(crate) struct MqttConfigDto {
    /// The address mosquitto binds to for internal use
    #[doku(example = "127.0.0.1")]
    pub(crate) bind_address: Option<IpAddress>,

    /// The port mosquitto binds to for internal use
    #[doku(example = "1883", as = "u16")]
    pub(crate) port: Option<u16>,

    /// The host that the thin-edge MQTT client should connect to
    #[doku(example = "localhost")]
    pub(crate) client_host: Option<String>,

    // TODO should these default to the configured bind settings rather than separate defaults?
    /// The port that the thin-edge MQTT client should connect to
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
    pub(crate) client_port: Option<Fakeable<NonZeroU16>>,

    /// Path to the CA certificate used by MQTT clients to use when authenticating the MQTT broker.
    #[doku(example = "/etc/mosquitto/ca_certificates/ca.crt", as = "PathBuf")]
    pub(crate) client_ca_file: Option<Fakeable<Utf8PathBuf>>,

    /// Path to the directory containing the CA certificates used by MQTT
    /// clients when authenticating the MQTT broker.
    #[doku(example = "/etc/mosquitto/ca_certificates", as = "PathBuf")]
    pub(crate) client_ca_path: Option<Fakeable<Utf8PathBuf>>,

    /// MQTT client authentication configuration, containing a path to a client
    /// certificate and a private key.
    #[serde(skip_serializing_if = "MqttClientAuthConfig::is_empty", default)]
    pub(crate) client_auth: MqttClientAuthConfig,

    /// The port mosquitto binds to for external use
    #[doku(example = "8883")]
    pub(crate) external_port: Option<u16>,

    /// The address mosquitto binds to for external use
    #[doku(example = "0.0.0.0")]
    pub(crate) external_bind_address: Option<IpAddress>,

    /// Name of network interface which the mqtt broker limits incoming connections on.
    #[doku(example = "wlan0")]
    pub(crate) external_bind_interface: Option<String>,

    // All the paths relating to mosquitto are strings as they need to be safe
    // to write to a configuration file (i.e. probably valid utf-8 at the least)
    /// Path to a file containing the PEM encoded CA certificates that are
    /// trusted when checking incoming client certificates
    #[doku(example = "/etc/ssl/certs", as = "PathBuf")]
    #[serde(alias = "external_capath")]
    pub(crate) external_ca_path: Option<Fakeable<Utf8PathBuf>>,

    /// Path to the certificate file which is used by the external MQTT listener
    #[doku(
        example = "/etc/tedge/device-certs/tedge-certificate.pem",
        as = "PathBuf",
        meta("note = This setting shall be used together with `mqtt.external_key_file` for external connections."),
    )]
    #[serde(alias = "external_certfile")]
    pub(crate) external_cert_file: Option<Fakeable<Utf8PathBuf>>,

    /// Path to the key file which is used by the external MQTT listener
    #[doku(
        example = "/etc/tedge/device-certs/tedge-private-key.pem",
        as = "PathBuf",
        meta("note = This setting shall be used together with `mqtt.external_cert_file` for external connections."),
    )]
    #[serde(alias = "external_keyfile")]
    pub(crate) external_key_file: Option<Fakeable<Utf8PathBuf>>,
}

impl MqttConfigDto {
    pub(crate) fn bind_address(&self) -> IpAddress {
        self.bind_address.unwrap_or_default()
    }

    pub(crate) fn port(&self) -> u16 {
        self.port.unwrap_or(DEFAULT_MQTT_PORT)
    }

    pub(crate) fn client_host(&self) -> Cow<str> {
        Cow::Borrowed(self.client_host.as_deref().unwrap_or("localhost"))
    }

    pub(crate) fn client_port(&self) -> NonZeroU16 {
        self.client_port
            .map(|p| p.0)
            .unwrap_or_else(|| NonZeroU16::new(1883).unwrap())
    }

    pub(crate) fn client_ca_file(&self) -> Option<&Utf8Path> {
        self.client_ca_file.as_deref()
    }

    pub(crate) fn client_ca_path(&self) -> Option<&Utf8Path> {
        self.client_ca_path.as_deref()
    }

    pub(crate) fn client_auth(&self) -> &MqttClientAuthConfig {
        &self.client_auth
    }

    pub(crate) fn external_port(&self) -> Option<u16> {
        self.external_port
    }

    pub(crate) fn external_bind_address(&self) -> Option<IpAddress> {
        self.external_bind_address
    }

    pub(crate) fn external_ca_path(&self) -> Option<&Utf8Path> {
        self.external_ca_path.as_deref()
    }

    pub(crate) fn external_cert_file(&self) -> Option<&Utf8Path> {
        self.external_cert_file.as_deref()
    }

    pub(crate) fn external_key_file(&self) -> Option<&Utf8Path> {
        self.external_key_file.as_deref()
    }

    pub(crate) fn external_bind_interface(&self) -> Option<&str> {
        self.external_bind_interface.as_deref()
    }
}

#[derive(Debug, Default, Deserialize, Serialize, Document, PartialEq, Eq)]
#[cfg_attr(test, derive(fake::Dummy))]
pub(crate) struct HttpConfigDto {
    /// HTTP server port used by the File Transfer Service
    #[doku(example = "8000")]
    #[serde(alias = "bind_port")]
    pub(crate) port: Option<u16>,
}

impl HttpConfigDto {
    pub(crate) fn port(&self) -> u16 {
        self.port.unwrap_or(8000)
    }
}

#[derive(Debug, Default, Deserialize, Serialize, Document, PartialEq, Eq)]
#[cfg_attr(test, derive(fake::Dummy))]
pub struct SoftwareConfigDto {
    /// The default software plugin to be used for software management on the device
    #[doku(example = "apt")]
    #[serde(alias = "plugin_default", alias = "default_plugin_type")]
    pub(crate) default_plugin: Option<String>,
}

impl SoftwareConfigDto {
    pub(crate) fn default_plugin(&self) -> Option<&str> {
        self.default_plugin.as_deref()
    }
}

#[derive(Debug, Default, Deserialize, Serialize, Document, PartialEq, Eq)]
#[cfg_attr(test, derive(fake::Dummy))]
pub struct LockPathConfigDto {
    /// The directory used to store runtime information, such as file locks
    #[serde(rename = "path")]
    #[doku(example = "/run", as = "PathBuf")]
    pub(crate) path: Option<Fakeable<Utf8PathBuf>>,

    /// Whether to create a lock file or not
    #[doku(example = "true")]
    pub(crate) lock_files: Option<bool>,
}

impl LockPathConfigDto {
    pub(crate) fn path(&self) -> &Utf8Path {
        self.path
            .as_deref()
            .unwrap_or_else(|| Utf8Path::new("/run"))
    }

    pub(crate) fn lock_files(&self) -> bool {
        self.lock_files.unwrap_or(true)
    }
}

#[derive(Debug, Default, Deserialize, Serialize, Document, PartialEq, Eq)]
#[cfg_attr(test, derive(fake::Dummy))]
pub struct TmpPathConfigDto {
    /// The temporary directory used to download files to the device
    #[serde(rename = "path")]
    #[doku(example = "/tmp", as = "PathBuf")]
    pub(crate) path: Option<Fakeable<Utf8PathBuf>>,
}

#[derive(Debug, Default, Deserialize, Serialize, Document, PartialEq, Eq)]
#[cfg_attr(test, derive(fake::Dummy))]
pub struct LogsPathConfigDto {
    /// The directory used to store logs.
    #[serde(rename = "path")]
    #[doku(example = "/var/log", as = "PathBuf")]
    pub(crate) path: Option<Fakeable<Utf8PathBuf>>,
}

#[derive(Debug, Default, Deserialize, Serialize, Document, PartialEq, Eq)]
#[cfg_attr(test, derive(fake::Dummy))]
pub struct DataPathConfigDto {
    /// The directory used to store data like cached files, runtime metadata etc.
    #[serde(rename = "path")]
    #[doku(example = "/var/tedge", as = "PathBuf")]
    pub(crate) path: Option<Fakeable<Utf8PathBuf>>,
}

impl DataPathConfigDto {
    pub(crate) fn path(&self) -> &Utf8Path {
        self.path
            .as_deref()
            .unwrap_or_else(|| Utf8Path::new("/var/tedge"))
    }
}

impl LogsPathConfigDto {
    pub(crate) fn path(&self) -> &Utf8Path {
        self.path
            .as_deref()
            .unwrap_or_else(|| Utf8Path::new("/var/log"))
    }
}

impl TmpPathConfigDto {
    pub(crate) fn path(&self) -> &Utf8Path {
        self.path
            .as_deref()
            .unwrap_or_else(|| Utf8Path::new("/tmp"))
    }
}

#[derive(Debug, Default, Deserialize, Serialize, Document, PartialEq, Eq)]
#[cfg_attr(test, derive(fake::Dummy))]
pub struct FirmwareConfigDto {
    /// The timeout limit in seconds for firmware update operations on child devices
    #[doku(example = "3600")]
    pub(crate) child_update_timeout: Option<u64>,
}

impl FirmwareConfigDto {
    pub(crate) fn child_update_timeout(&self) -> Duration {
        Duration::from_secs(self.raw_child_update_timeout())
    }

    pub(crate) fn raw_child_update_timeout(&self) -> u64 {
        self.child_update_timeout.unwrap_or(3600)
    }
}

#[derive(Debug, Default, Deserialize, Serialize, Document, PartialEq, Eq)]
#[cfg_attr(test, derive(fake::Dummy))]
pub struct ServiceTypeConfigDto {
    /// The thin-edge.io service's service type
    #[serde(rename = "type")]
    #[doku(example = "systemd")]
    pub(crate) service_type: Option<String>,
}

impl ServiceTypeConfigDto {
    pub(crate) fn service_type(&self) -> &str {
        self.service_type.as_deref().unwrap_or("service")
    }
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
#[derive(Debug, Default, Deserialize, Serialize, Document, PartialEq, Eq)]
#[cfg_attr(test, derive(fake::Dummy))]
pub(crate) struct MqttClientAuthConfig {
    /// Path to the client certificate
    #[doku(example = "/path/to/client.crt", as = "PathBuf")]
    pub cert_file: Option<Fakeable<Utf8PathBuf>>,

    /// Path to the client private key
    #[doku(example = "/path/to/client.key", as = "PathBuf")]
    pub key_file: Option<Fakeable<Utf8PathBuf>>,
}

impl MqttClientAuthConfig {
    fn is_empty(&self) -> bool {
        self == &MqttClientAuthConfig::default()
    }

    pub(crate) fn cert_file(&self) -> Option<&Utf8Path> {
        self.cert_file.as_deref()
    }

    pub(crate) fn key_file(&self) -> Option<&Utf8Path> {
        self.key_file.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use core::panic;
    use figment::providers::Format;
    use serde::de::DeserializeOwned;

    use crate::tedge_config_cli::new_tedge_config::struct_field_paths;

    use super::*;

    fn writable_keys() -> Vec<(Cow<'static, str>, doku::Type)> {
        let ty = TEdgeConfigDto::ty();
        let doku::TypeKind::Struct { fields, transparent: false } = ty.kind else { panic!("Expected struct but got {:?}", ty.kind) };
        let doku::Fields::Named { fields } = fields else { panic!("Expected named fields but got {:?}", fields)};
        struct_field_paths(None, &fields)
    }

    #[test]
    fn example_values_can_be_deserialised() {
        for (key, ty) in writable_keys() {
            verify_examples_for::<TEdgeConfigDto>(&key, &ty)
        }
    }

    fn verify_examples_for<Dto>(key: &str, ty: &doku::Type)
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
}
