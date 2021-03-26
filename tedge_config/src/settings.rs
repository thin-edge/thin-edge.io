use crate::{config_setting::*, types::*};

///
/// Identifier of the device within the fleet. It must be globally
/// unique and the same one used in the device certificate.
///
/// Example: Raspberrypi-4d18303a-6d3a-11eb-b1a6-175f6bb72665")
///
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct DeviceIdSetting;

impl ConfigSetting for DeviceIdSetting {
    const EXTERNAL_KEY: &'static str = "device.id";

    const DESCRIPTION: &'static str = concat!(
        "Identifier of the device within the fleet. It must be ",
        "globally unique and the same one used in the device certificate. ",
        "Example: Raspberrypi-4d18303a-6d3a-11eb-b1a6-175f6bb72665"
    );

    type Value = String;
}

///
/// Path to the private key file. Example: /home/user/.tedge/tedge-private-key.pem
///
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct DeviceKeyPathSetting;

impl ConfigSetting for DeviceKeyPathSetting {
    const EXTERNAL_KEY: &'static str = "device.key.path";

    const DESCRIPTION: &'static str =
        "Path to the private key file. Example: /home/user/.tedge/tedge-private-key.pem";

    type Value = String;
}

///
/// Path to the certificate file.
///
/// Example: /home/user/.tedge/tedge-certificate.crt
///
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct DeviceCertPathSetting;

impl ConfigSetting for DeviceCertPathSetting {
    const EXTERNAL_KEY: &'static str = "device.cert.path";

    const DESCRIPTION: &'static str =
        "Path to the certificate file. Example: /home/user/.tedge/tedge-certificate.crt";

    type Value = String;
}

///
/// Tenant endpoint URL of Cumulocity tenant.
///
/// Example: your-tenant.cumulocity.com
///
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct C8yUrlSetting;

impl ConfigSetting for C8yUrlSetting {
    const EXTERNAL_KEY: &'static str = "c8y.url";

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
    const EXTERNAL_KEY: &'static str = "c8y.root_cert_path";

    const DESCRIPTION: &'static str = concat!(
        "Path where Cumulocity root certificate(s) are located. ",
        "Example: /home/user/.tedge/c8y-trusted-root-certificates.pem"
    );

    type Value = String;
}

///
/// Tenant endpoint URL of Azure IoT tenant.
///
/// Example: MyAzure.azure-devices.net
///
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct AzureUrlSetting;

impl ConfigSetting for AzureUrlSetting {
    const EXTERNAL_KEY: &'static str = "device.id";

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
    const EXTERNAL_KEY: &'static str = "azure.root_cert_path";

    const DESCRIPTION: &'static str = concat!(
        "Path where Azure IoT root certificate(s) are located. ",
        "Example: /home/user/.tedge/azure-trusted-root-certificates.pem"
    );

    type Value = String;
}
