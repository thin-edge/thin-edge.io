use crate::tedge_config_cli::config_setting::*;
use crate::tedge_config_cli::models::*;
use camino::Utf8PathBuf;

///
/// Identifier of the device within the fleet. It must be globally
/// unique and the same one used in the device certificate.
/// NOTE: This setting is derived from the device certificate and therefore is read only.
///
/// Example: Raspberrypi-4d18303a-6d3a-11eb-b1a6-175f6bb72665")
///
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct DeviceIdSetting;

impl ConfigSetting for DeviceIdSetting {
    const KEY: &'static str = "device.id";

    const DESCRIPTION: &'static str = concat!(
        "Identifier of the device within the fleet. It must be ",
        "globally unique and is derived from the device certificate. ",
        "Example: Raspberrypi-4d18303a-6d3a-11eb-b1a6-175f6bb72665",
        "NOTE: This setting is derived from the device certificate and therefore is read only."
    );

    type Value = String;
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct DeviceTypeSetting;

impl ConfigSetting for DeviceTypeSetting {
    const KEY: &'static str = "device.type";

    const DESCRIPTION: &'static str = "The default device type. Example: thin-edge.io";

    type Value = String;
}

///
/// Path to the private key file. Example: /home/user/.tedge/tedge-private-key.pem
///
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct DeviceKeyPathSetting;

impl ConfigSetting for DeviceKeyPathSetting {
    const KEY: &'static str = "device.key_path";

    const DESCRIPTION: &'static str =
        "Path to the private key file. Example: /home/user/.tedge/tedge-private-key.pem";

    type Value = Utf8PathBuf;
}

///
/// Path to the certificate file.
///
/// Example: /home/user/.tedge/tedge-certificate.crt
///
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct DeviceCertPathSetting;

impl ConfigSetting for DeviceCertPathSetting {
    const KEY: &'static str = "device.cert_path";

    const DESCRIPTION: &'static str =
        "Path to the certificate file. Example: /home/user/.tedge/tedge-certificate.crt";

    type Value = Utf8PathBuf;
}

///
/// Tenant endpoint URL of Cumulocity tenant.
///
/// Example: your-tenant.cumulocity.com
///
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[deprecated = "Use `C8yHttpSetting` or `C8yMqttSetting` instead"]
pub struct C8yUrlSetting;

#[allow(deprecated)]
impl ConfigSetting for C8yUrlSetting {
    const KEY: &'static str = "c8y.url";

    const DESCRIPTION: &'static str =
        "Tenant endpoint URL of Cumulocity tenant. Example: your-tenant.cumulocity.com";
    type Value = ConnectUrl;
}

/// HTTP endpoint for the Cumulocity tenant.
///
/// Example: http.your-tenant.cumulocity.com:1234
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct C8yHttpSetting;

impl ConfigSetting for C8yHttpSetting {
    const KEY: &'static str = "c8y.http";

    const DESCRIPTION: &'static str = "HTTP endpoint for the Cumulocity tenant. \
        Example: http.your-tenant.cumulocity.com:1234";
    type Value = HostPort<HTTPS_PORT>;
}

/// MQTT endpoint for the Cumulocity tenant.
///
/// Example: mqtt.your-tenant.cumulocity.com:1234
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct C8yMqttSetting;

impl ConfigSetting for C8yMqttSetting {
    const KEY: &'static str = "c8y.mqtt";

    const DESCRIPTION: &'static str = "MQTT endpoint for the Cumulocity tenant. \
        Example: mqtt.your-tenant.cumulocity.com:1234";
    type Value = HostPort<MQTT_TLS_PORT>;
}

///
/// Path where Cumulocity root certificate(s) are located.
///
/// Example: /home/user/.tedge/c8y-trusted-root-certificates.pem
///
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct C8yRootCertPathSetting;

impl ConfigSetting for C8yRootCertPathSetting {
    const KEY: &'static str = "c8y.root_cert_path";

    const DESCRIPTION: &'static str = concat!(
        "Path where Cumulocity root certificate(s) are located. ",
        "Example: /home/user/.tedge/c8y-trusted-root-certificates.pem"
    );

    type Value = Utf8PathBuf;
}

///
/// Smartrest templates to subscribe to.
///
/// Example: template1,template2
///
#[derive(Debug)]
pub struct C8ySmartRestTemplates;

impl ConfigSetting for C8ySmartRestTemplates {
    const KEY: &'static str = "c8y.smartrest.templates";

    const DESCRIPTION: &'static str = concat!(
        "Set of SmartRest templates for the device ",
        "Example: template1,template2"
    );

    type Value = TemplatesSet;
}

///
/// Tenant endpoint URL of Azure IoT tenant.
///
/// Example: MyAzure.azure-devices.net
///
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct AzureUrlSetting;

impl ConfigSetting for AzureUrlSetting {
    const KEY: &'static str = "az.url";

    const DESCRIPTION: &'static str = concat!(
        "Tenant endpoint URL of Azure IoT tenant. ",
        "Example:  MyAzure.azure-devices.net"
    );

    type Value = ConnectUrl;
}

///
/// Path where Azure IoT root certificate(s) are located.
///
/// Example: /home/user/.tedge/azure-trusted-root-certificates.pem
///
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct AzureRootCertPathSetting;

impl ConfigSetting for AzureRootCertPathSetting {
    const KEY: &'static str = "az.root_cert_path";

    const DESCRIPTION: &'static str = concat!(
        "Path where Azure IoT root certificate(s) are located. ",
        "Example: /home/user/.tedge/azure-trusted-root-certificates.pem"
    );

    type Value = Utf8PathBuf;
}

///
/// Boolean whether Azure mapper should add timestamp if timestamp is not added in the incoming payload.
///
/// Example: true
///
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct AzureMapperTimestamp;

impl ConfigSetting for AzureMapperTimestamp {
    const KEY: &'static str = "az.mapper.timestamp";

    const DESCRIPTION: &'static str = concat!(
        "Boolean whether Azure mapper should add timestamp or not. ",
        "Example: true"
    );

    type Value = Flag;
}

///
/// Boolean whether AWS mapper should add timestamp if timestamp is not added in the incoming payload.
///
/// Example: true
///
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct AwsMapperTimestamp;

impl ConfigSetting for AwsMapperTimestamp {
    const KEY: &'static str = "aws.mapper.timestamp";

    const DESCRIPTION: &'static str = concat!(
        "Boolean whether AWS mapper should add timestamp or not. ",
        "Example: true"
    );

    type Value = Flag;
}

///
/// Endpoint URL of AWS instance.
///
/// Example: your-endpoint.amazonaws.com
///
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct AwsUrlSetting;

impl ConfigSetting for AwsUrlSetting {
    const KEY: &'static str = "aws.url";

    const DESCRIPTION: &'static str =
        "Endpoint URL of AWS instance. Example: your-endpoint.amazonaws.com";
    type Value = ConnectUrl;
}

///
/// Path where AWS IoT root certificate(s) are located.
///
/// Example: /home/user/.tedge/aws-trusted-root-certificates.pem
///
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct AwsRootCertPathSetting;

impl ConfigSetting for AwsRootCertPathSetting {
    const KEY: &'static str = "aws.root_cert_path";

    const DESCRIPTION: &'static str = concat!(
        "Path where AWS IoT root certificate(s) are located. ",
        "Example: /home/user/.tedge/aws-trusted-root-certificates.pem"
    );

    type Value = Utf8PathBuf;
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct MqttClientHostSetting;

impl ConfigSetting for MqttClientHostSetting {
    const KEY: &'static str = "mqtt.client.host";

    const DESCRIPTION: &'static str = concat!(
        "Mqtt broker address, which is used by the mqtt clients to publish or subscribe.",
        "Example: 127.0.0.1"
    );

    type Value = String;
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct MqttClientPortSetting;

impl ConfigSetting for MqttClientPortSetting {
    const KEY: &'static str = "mqtt.client.port";

    const DESCRIPTION: &'static str = concat!(
        "Mqtt broker port, which is used by the mqtt clients to publish or subscribe.",
        "Example: 1883"
    );

    type Value = Port;
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct MqttClientCafileSetting;

impl ConfigSetting for MqttClientCafileSetting {
    const KEY: &'static str = "mqtt.client.auth.ca_file";

    const DESCRIPTION: &'static str = concat!(
        "Path to the CA certificate used by MQTT clients to use when ",
        "authenticating the MQTT broker."
    );

    type Value = Utf8PathBuf;
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct MqttClientCapathSetting;

impl ConfigSetting for MqttClientCapathSetting {
    const KEY: &'static str = "mqtt.client.auth.ca_dir";

    const DESCRIPTION: &'static str = concat!(
        "Path to the directory containing CA certificate used by MQTT clients ",
        "to use when authenticating the MQTT broker. For a certificate to be ",
        "used, it needs to have one of the following extensions: .pem/.crt/.cer"
    );

    type Value = Utf8PathBuf;
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct MqttClientAuthCertSetting;

impl ConfigSetting for MqttClientAuthCertSetting {
    const KEY: &'static str = "mqtt.client.auth.cert_file";

    const DESCRIPTION: &'static str =
        "Path to the client certificate used for client authentication";

    type Value = Utf8PathBuf;
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct MqttClientAuthKeySetting;

impl ConfigSetting for MqttClientAuthKeySetting {
    const KEY: &'static str = "mqtt.client.auth.key_file";

    const DESCRIPTION: &'static str =
        "Path to the client private key used for client authentication";

    type Value = Utf8PathBuf;
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct MqttPortSetting;

impl ConfigSetting for MqttPortSetting {
    const KEY: &'static str = "mqtt.bind.port";

    const DESCRIPTION: &'static str = concat!(
        "Mqtt broker port, which is used by the local mqtt clients to publish or subscribe. ",
        "Example: 1883"
    );

    type Value = Port;
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct HttpPortSetting;

impl ConfigSetting for HttpPortSetting {
    const KEY: &'static str = "http.port";

    const DESCRIPTION: &'static str = concat!(
        "Http client port, which is used by the File Transfer Service",
        "Example: 8000"
    );

    type Value = Port;
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct HttpBindAddressSetting;

impl ConfigSetting for HttpBindAddressSetting {
    const KEY: &'static str = "http.address";

    const DESCRIPTION: &'static str = concat!(
        "Http client address, which is used by the File Transfer Service",
        "Example: 127.0.0.1, 192.168.1.2"
    );

    type Value = IpAddress;
}

pub struct MqttBindAddressSetting;

impl ConfigSetting for MqttBindAddressSetting {
    const KEY: &'static str = "mqtt.bind.address";

    const DESCRIPTION: &'static str = concat!(
        "Mqtt bind address, which is used by the mqtt clients to publish or subscribe. ",
        "Example: 127.0.0.1"
    );

    type Value = IpAddress;
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct MqttExternalPortSetting;

impl ConfigSetting for MqttExternalPortSetting {
    const KEY: &'static str = "mqtt.external.bind.port";

    const DESCRIPTION: &'static str = concat!(
        "Mqtt broker port, which is used by the external mqtt clients to publish or subscribe. ",
        "Example: 8883"
    );

    type Value = Port;
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct MqttExternalBindAddressSetting;

impl ConfigSetting for MqttExternalBindAddressSetting {
    const KEY: &'static str = "mqtt.external.bind.address";

    const DESCRIPTION: &'static str = concat!(
        "IP address / hostname, which the mqtt broker limits incoming connections on. ",
        "Example: 0.0.0.0"
    );

    type Value = IpAddress;
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct MqttExternalBindInterfaceSetting;

impl ConfigSetting for MqttExternalBindInterfaceSetting {
    const KEY: &'static str = "mqtt.external.bind.interface";

    const DESCRIPTION: &'static str = concat!(
        "Name of network interface, which the mqtt broker limits incoming connections on. ",
        "Example: wlan0"
    );

    type Value = String;
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct MqttExternalCAPathSetting;

impl ConfigSetting for MqttExternalCAPathSetting {
    const KEY: &'static str = "mqtt.external.ca_path";

    const DESCRIPTION: &'static str = concat!(
        "Path to a file containing the PEM encoded CA certificates ",
        "that are trusted when checking incoming client certificates. ",
        "Example: /etc/ssl/certs",
        "Note: If the capath is not set, then no certificates are required for the external connections."
    );

    type Value = Utf8PathBuf;
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct MqttExternalCertfileSetting;

impl ConfigSetting for MqttExternalCertfileSetting {
    const KEY: &'static str = "mqtt.external.cert_file";

    const DESCRIPTION: &'static str = concat!(
        "Path to the certificate file, which is used by external MQTT listener",
        "Example: /etc/tedge/device-certs/tedge-certificate.pem",
        "Note: This setting shall be used together with `mqtt.external.key_file` for external connections."
    );

    type Value = Utf8PathBuf;
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct MqttExternalKeyfileSetting;

impl ConfigSetting for MqttExternalKeyfileSetting {
    const KEY: &'static str = "mqtt.external.key_file";

    const DESCRIPTION: &'static str = concat!(
        "Path to the private key file, which is used by external MQTT listener",
        "Example: /etc/tedge/device-certs/tedge-private-key.pem",
        "Note: This setting shall be used together with `mqtt.external.cert_file` for external connections."
    );

    type Value = Utf8PathBuf;
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct SoftwarePluginDefaultSetting;

impl ConfigSetting for SoftwarePluginDefaultSetting {
    const KEY: &'static str = "software.plugin.default";

    const DESCRIPTION: &'static str = concat!(
        "The default software plugin to be used for software management on the device",
        "Example: apt"
    );

    type Value = String;
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct TmpPathSetting;

impl ConfigSetting for TmpPathSetting {
    const KEY: &'static str = "tmp.path";

    const DESCRIPTION: &'static str = concat!(
        "The temporary directory path to be used for downloads on the device",
        "Example: /tmp"
    );

    type Value = Utf8PathBuf;
}

pub struct LogPathSetting;

impl ConfigSetting for LogPathSetting {
    const KEY: &'static str = "logs.path";

    const DESCRIPTION: &'static str = concat!(
        "The directory path to be used for logs",
        "Example: /var/log"
    );

    type Value = Utf8PathBuf;
}

pub struct RunPathSetting;

impl ConfigSetting for RunPathSetting {
    const KEY: &'static str = "run.path";

    const DESCRIPTION: &'static str = concat!(
        "The directory path to be used for runtime information",
        "Example: /run"
    );

    type Value = Utf8PathBuf;
}

pub struct DataPathSetting;

impl ConfigSetting for DataPathSetting {
    const KEY: &'static str = "data.path";

    const DESCRIPTION: &'static str = concat!(
        "The directory path to be used to store data like cached files, runtime metadata etc.",
        "Example: /var/tedge"
    );

    type Value = Utf8PathBuf;
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct LockFilesSetting;

impl ConfigSetting for LockFilesSetting {
    const KEY: &'static str = "run.lock_files";

    const DESCRIPTION: &'static str = concat!(
        "Boolean whether lock files should be created or not.",
        "Example: true"
    );

    type Value = Flag;
}

pub struct FirmwareChildUpdateTimeoutSetting;

impl ConfigSetting for FirmwareChildUpdateTimeoutSetting {
    const KEY: &'static str = "firmware.child.update.timeout";

    const DESCRIPTION: &'static str = concat!(
        "The timeout limit in seconds for firmware update operations on child devices",
        "Example: 3600"
    );

    type Value = Seconds;
}

pub struct ServiceTypeSetting;

impl ConfigSetting for ServiceTypeSetting {
    const KEY: &'static str = "service.type";

    const DESCRIPTION: &'static str = concat!(
        "The thin-edge.io service's service type",
        "Example: systemd"
    );

    type Value = String;
}
