pub use self::{azure_url::*, c8y_url::*, device_id::*};

mod azure_url;
mod c8y_url;
mod device_id;

#[derive(thiserror::Error, Debug)]
pub enum ConfigSettingError {
    #[error(
        r#"A value for `{key}` is missing.
    A value can be set with `tedge config set {key} <value>`"#
    )]
    ConfigNotSet { key: &'static str },

    #[error(
        r#"Provided URL: '{0}' contains scheme or port.
    Provided URL should contain only domain, eg: 'subdomain.cumulocity.com'."#
    )]
    InvalidConfigUrl(String),
}

pub type ConfigSettingResult<T> = Result<T, ConfigSettingError>;

pub trait GetConfigSetting {
    type Config;
    type Value;

    fn get(&self, config: &Self::Config) -> ConfigSettingResult<Self::Value>;
}

pub trait GetStringConfigSetting: std::fmt::Debug {
    type Config;

    fn get_string(&self, config: &Self::Config) -> ConfigSettingResult<String>;

    fn get_string_or_default(
        &self,
        config: &Self::Config,
        default: &str,
    ) -> ConfigSettingResult<String> {
        match self.get_string(config) {
            Ok(s) => Ok(s),
            Err(ConfigSettingError::ConfigNotSet { .. }) => Ok(default.into()),
            Err(err) => Err(err),
        }
    }
}

pub trait SetStringConfigSetting: std::fmt::Debug {
    type Config;

    fn set_string(&self, config: &mut Self::Config, value: String) -> ConfigSettingResult<()>;
}

pub trait UnsetConfigSetting: std::fmt::Debug {
    type Config;

    fn unset(&self, config: &mut Self::Config) -> ConfigSettingResult<()>;
}

macro_rules! impl_get_setting {
    ($ty:ident, $( $key:ident ).*, $path:literal) => {
        impl GetStringConfigSetting for $ty {
            type Config = crate::config::TEdgeConfig;

            fn get_string(&self, config: &Self::Config) -> ConfigSettingResult<String> {
                config.
                    $( $key ).*
                    .clone()
                    .ok_or(ConfigSettingError::ConfigNotSet { key: $path })
            }
        }
    }
}

macro_rules! impl_set_setting {
    ($ty:ident, $( $key:ident ).*, $path:literal) => {
        impl SetStringConfigSetting for $ty {
            type Config = crate::config::TEdgeConfig;

            fn set_string(&self, config: &mut Self::Config, value: String) -> ConfigSettingResult<()> {
                config.
                    $( $key ).*  = Some(value.into());
                Ok(())
            }
        }
    }
}

macro_rules! impl_unset_setting {
    ($ty:ident, $( $key:ident ).*, $path:literal) => {
        impl UnsetConfigSetting for $ty {
            type Config = crate::config::TEdgeConfig;

            fn unset(&self, config: &mut Self::Config) -> ConfigSettingResult<()> {
                config.
                    $( $key ).*  = None;
                Ok(())
            }
        }
    }
}

macro_rules! make_rw_setting {
    ($ty:ident, $( $key:ident ).*, $path:literal) => {
        #[derive(Debug)]
        pub struct $ty;

        impl_get_setting!($ty, $($key).*, $path);
        impl_set_setting!($ty, $($key).*, $path);
        impl_unset_setting!($ty, $($key).*, $path);
    }
}

// Path to the private key file. Example: /home/user/.tedge/tedge-private-key.pem
make_rw_setting!(DeviceKeyPathSetting, device.key_path, "device.key.path");

// Path to the certificate file. Example: /home/user/.tedge/tedge-certificate.crt
make_rw_setting!(DeviceCertPathSetting, device.cert_path, "device.cert.path");

// Path where Cumulocity root certificate(s) are located. Example: /home/user/.tedge/c8y-trusted-root-certificates.pem
make_rw_setting!(
    C8yRootCertPathSetting,
    c8y.root_cert_path,
    "c8y.root.cert.path"
);

// Path where Azure IoT root certificate(s) are located. Example: /home/user/.tedge/azure-trusted-root-certificates.pem
make_rw_setting!(
    AzureRootCertPathSetting,
    azure.root_cert_path,
    "azure.root.cert.path"
);
