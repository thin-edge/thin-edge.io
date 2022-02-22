use crate::{config_setting::*, models::*};

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
    const KEY: &'static str = "device.key.path";

    const DESCRIPTION: &'static str =
        "Path to the private key file. Example: /home/user/.tedge/tedge-private-key.pem";

    type Value = FilePath;
}

///
/// Path to the certificate file.
///
/// Example: /home/user/.tedge/tedge-certificate.crt
///
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct DeviceCertPathSetting;

impl ConfigSetting for DeviceCertPathSetting {
    const KEY: &'static str = "device.cert.path";

    const DESCRIPTION: &'static str =
        "Path to the certificate file. Example: /home/user/.tedge/tedge-certificate.crt";

    type Value = FilePath;
}

///
/// Tenant endpoint URL of Cumulocity tenant.
///
/// Example: your-tenant.cumulocity.com
///
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct C8yUrlSetting;

impl ConfigSetting for C8yUrlSetting {
    const KEY: &'static str = "c8y.url";

    const DESCRIPTION: &'static str =
        "Tenant endpoint URL of Cumulocity tenant. Example: your-tenant.cumulocity.com";
    type Value = ConnectUrl;
}

///
/// Path where Cumulocity root certificate(s) are located.
///
/// Example: /home/user/.tedge/c8y-trusted-root-certificates.pem
///
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct C8yRootCertPathSetting;

impl ConfigSetting for C8yRootCertPathSetting {
    const KEY: &'static str = "c8y.root.cert.path";

    const DESCRIPTION: &'static str = concat!(
        "Path where Cumulocity root certificate(s) are located. ",
        "Example: /home/user/.tedge/c8y-trusted-root-certificates.pem"
    );

    type Value = FilePath;
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
    const KEY: &'static str = "az.root.cert.path";

    const DESCRIPTION: &'static str = concat!(
        "Path where Azure IoT root certificate(s) are located. ",
        "Example: /home/user/.tedge/azure-trusted-root-certificates.pem"
    );

    type Value = FilePath;
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

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct MqttPortSetting;

impl ConfigSetting for MqttPortSetting {
    const KEY: &'static str = "mqtt.port";

    const DESCRIPTION: &'static str = concat!(
        "Mqtt broker port, which is used by the local mqtt clients to publish or subscribe. ",
        "Example: 1883"
    );

    type Value = Port;
}

pub struct MqttBindAddressSetting;

impl ConfigSetting for MqttBindAddressSetting {
    const KEY: &'static str = "mqtt.bind_address";

    const DESCRIPTION: &'static str = concat!(
        "Mqtt bind address, which is used by the mqtt clients to publish or subscribe. ",
        "Example: 0.0.0.0"
    );

    type Value = String;
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct MqttExternalPortSetting;

impl ConfigSetting for MqttExternalPortSetting {
    const KEY: &'static str = "mqtt.external.port";

    const DESCRIPTION: &'static str = concat!(
        "Mqtt broker port, which is used by the external mqtt clients to publish or subscribe. ",
        "Example: 8883"
    );

    type Value = Port;
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct MqttExternalBindAddressSetting;

impl ConfigSetting for MqttExternalBindAddressSetting {
    const KEY: &'static str = "mqtt.external.bind_address";

    const DESCRIPTION: &'static str = concat!(
        "IP address / hostname, which the mqtt broker limits incoming connections on. ",
        "Example: 0.0.0.0"
    );

    type Value = String;
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct MqttExternalBindInterfaceSetting;

impl ConfigSetting for MqttExternalBindInterfaceSetting {
    const KEY: &'static str = "mqtt.external.bind_interface";

    const DESCRIPTION: &'static str = concat!(
        "Name of network interface, which the mqtt broker limits incoming connections on. ",
        "Example: wlan0"
    );

    type Value = String;
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct MqttExternalCAPathSetting;

impl ConfigSetting for MqttExternalCAPathSetting {
    const KEY: &'static str = "mqtt.external.capath";

    const DESCRIPTION: &'static str = concat!(
        "Path to a file containing the PEM encoded CA certificates ",
        "that are trusted when checking incoming client certificates. ",
        "Example: /etc/ssl/certs",
        "Note: If the capath is not set, then no certificates are required for the external connections."
    );

    type Value = FilePath;
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct MqttExternalCertfileSetting;

impl ConfigSetting for MqttExternalCertfileSetting {
    const KEY: &'static str = "mqtt.external.certfile";

    const DESCRIPTION: &'static str = concat!(
        "Path to the certificate file, which is used by external MQTT listener",
        "Example: /etc/tedge/device-certs/tedge-certificate.pem",
        "Note: This setting shall be used together with `mqtt.external.keyfile` for external connections."
    );

    type Value = FilePath;
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct MqttExternalKeyfileSetting;

impl ConfigSetting for MqttExternalKeyfileSetting {
    const KEY: &'static str = "mqtt.external.keyfile";

    const DESCRIPTION: &'static str = concat!(
        "Path to the private key file, which is used by external MQTT listener",
        "Example: /etc/tedge/device-certs/tedge-private-key.pem",
        "Note: This setting shall be used together with `mqtt.external.certfile` for external connections."
    );

    type Value = FilePath;
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
pub struct TmpPathDefaultSetting;

impl ConfigSetting for TmpPathDefaultSetting {
    const KEY: &'static str = "tmp.path";

    const DESCRIPTION: &'static str = concat!(
        "The default temporary path to be used for downloads on the device",
        "Example: /tmp"
    );

    type Value = FilePath;
}
