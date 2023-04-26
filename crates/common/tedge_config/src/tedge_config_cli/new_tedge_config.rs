//! A collection of tools for reading from and writing to tedge.toml
use std::borrow::Cow;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt;
use std::hash::Hash;
use std::num::NonZeroU16;
use std::sync::Mutex;
use std::time::Duration;

use camino::Utf8PathBuf;
use doku::Document;
use figment::providers::Serialized;
use figment::util::nest;
use figment::value::*;
use figment::*;
use once_cell::sync::Lazy;
use once_cell::sync::OnceCell;

use certificate::CertificateError;
use certificate::PemCertificate;

use crate::ConfigSettingError;
use crate::ConnectUrl;
use crate::IpAddress;
use crate::TEdgeConfigLocation;
use crate::TemplatesSet;

use super::tedge_config_dto::TEdgeConfigDto;

/// The configuration of thin-edge
///
/// This handles fetching values from
/// [TedgeConfigDto](super::tedge_config_dto::TEdgeConfigDto) as well as from
/// the system (see [device_id](Self::device_id)). It has getter methods for
/// each field, e.g. [c8y_url](Self::c8y_url), [logs_path](Self::logs_path) etc.
///
/// ```
/// # use tedge_test_utils::fs::TempTedgeDir;
/// use tedge_config::*;
///
/// let temp_dir = TempTedgeDir::new();
/// let config_repo = TEdgeConfigRepository::new(TEdgeConfigLocation::from_custom_root(temp_dir.path()));
///
/// let config = config_repo.load_new()?;
///
/// // Getters for settings with defaults are infallible
/// assert_eq!(config.mqtt_port(), 1883);
///
/// // Getters for other settings will fail if the value is not setting
/// assert!(config.mqtt_external_port().is_err());
///
/// // The defaults for some values are inferred from the config location
/// assert!(config.device_key_path().starts_with(temp_dir.utf8_path()));
///
/// # Ok::<_, TEdgeConfigError>(())
/// ```
pub struct NewTEdgeConfig {
    /// The configured values from tedge.toml or environment variables
    stored: TEdgeConfigDto,
    /// A cache for the device id, to save us re-reading certificates unneccessarily
    device_id: OnceCell<String>,
    /// The location of the configuration file
    config_location: TEdgeConfigLocation,
}

impl NewTEdgeConfig {
    pub(crate) fn new(value: crate::TEdgeConfig, location: &TEdgeConfigLocation) -> Self {
        Self {
            stored: value.data,
            device_id: <_>::default(),
            config_location: location.to_owned(),
        }
    }

    /// Read an arbitrary key as a string
    ///
    /// This is generally useful for the `tedge` CLI. Other crates should use
    /// the getter methods documented below, such as
    /// [device_id](Self::device_id), [c8y_url](Self::c8y_url) and
    /// [az_mapper_timestamp](Self::az_mapper_timestamp).
    pub fn read(&self, key: ReadableKey) -> Result<Option<String>, ConfigSettingError> {
        use ReadOnlyKey::*;
        use ReadableKey::*;

        Ok(match key {
            ReadOnly(DeviceId) => Some(self.device_id()?.to_string()),
            ReadOnly(HttpAddress) => Some(self.http_address().to_string()),
            ReadWrite(key) => self.read_configured_value_string(key),
        })
    }

    /// Reads `device.id` from the configured certificate
    ///
    /// # Errors
    /// This returns an error if the certificate path cannot be read
    /// ([ConfigSettingError::ReadOnlySettingNotConfigured]).
    ///
    /// It also returns an error if the PEM file cannot be parsed
    /// ([ConfigSettingError::DerivationFailed]).
    pub fn device_id(&self) -> Result<&str, ConfigSettingError> {
        self.device_id
            .get_or_try_init(|| {
                let cert_path = self.stored.device().cert_path(&self.config_location);
                PemCertificate::from_pem_file(&*cert_path)?.subject_common_name()
            })
            .map(|s| s.as_str())
            .map_err(|err| cert_error_into_config_error("device.id", err))
    }

    /// Getter for `http.address`
    pub fn http_address(&self) -> IpAddress {
        self.stored
            .mqtt()
            .external_bind_address()
            .unwrap_or_else(|| self.stored.mqtt().bind_address())
    }

    /// Getter for `firmware.child_update_timeout`
    pub fn firmware_child_update_timeout(&self) -> Duration {
        self.stored.firmware().child_update_timeout()
    }
}

fn cert_error_into_config_error(key: &'static str, err: CertificateError) -> ConfigSettingError {
    match &err {
        CertificateError::IoError(err) if err.kind() == std::io::ErrorKind::NotFound => {
            ConfigSettingError::ReadOnlySettingNotConfigured {
                key,
                message: "The device id is read from the device certificate.\n\
                To set 'device.id' to some <id>, you can use `tedge cert create --device-id <id>`.",
            }
        }
        _ => ConfigSettingError::DerivationFailed {
            key,
            cause: Box::new(err),
        },
    }
}

impl ReadOnlyKey {
    fn read_only_error(&self) -> &'static str {
        match self {
            Self::DeviceId => "\
                The device id is read from the device certificate and cannot be set directly.\n\
                To set 'device.id' to some <id>, you can use `tedge cert create --device-id <id>`.",
            Self::HttpAddress => "\
                The http address cannot be set directly. It is read from the mqtt bind address.\n\
                To set 'http.bind_address' to some <address>, you can `tedge config set mqtt.bind_address <address>`.", 
        }
    }
}

/// A [figment::Provider] for stringly-typed updates to [TEdgeConfigDto]
///
/// This takes a [WritableKey] and a value as a string slice.
#[must_use]
pub(crate) struct TEdgeConfigUpdate<'a> {
    key: WritableKey,
    value: &'a str,
}

impl<'a> TEdgeConfigUpdate<'a> {
    /// Creates a [TEdgeConfigUpdate] which represents updating the given key to the given value
    pub fn new(key: WritableKey, value: &'a str) -> Self {
        TEdgeConfigUpdate { key, value }
    }

    /// Applies this update to the provided dto, returning the updated dto
    ///
    /// # Errors
    /// This will fail if attempting to write to an unrecognised key or a
    /// read-only key
    pub fn apply_to(&self, config: &TEdgeConfigDto) -> Result<TEdgeConfigDto, ConfigSettingError> {
        Figment::new()
            .merge(Serialized::defaults(config))
            .merge(self)
            .extract()
            .map_err(|error| ConfigSettingError::Figment {
                key: self.key,
                error,
            })
    }
}

impl From<WritableKey> for ReadableKey {
    fn from(value: WritableKey) -> Self {
        Self::ReadWrite(value)
    }
}

impl<'a> Provider for TEdgeConfigUpdate<'a> {
    fn metadata(&self) -> figment::Metadata {
        Metadata::named(format!("tedge config value: {}", self.key))
            .interpolater(move |_: &Profile, k: &[&str]| normalize_key(&k.join(".")))
    }

    fn data(&self) -> Result<Map<Profile, Dict>, figment::Error> {
        let value = self.value.parse().expect("infallible");

        let dict = nest(self.key.as_str(), value)
            .into_dict()
            .expect("key is non-empty: must have dict");

        Ok(Profile::default().collect(dict))
    }
}

macro_rules! key_name_for {
    () => {};
    ($($ident:ident),+ ; $lit:literal) => {
        $lit
    };
    ($single:ident) => {
        stringify!($single)
    };
    ($first:ident, $($rest:ident),+) => {
        concat!(stringify!($first), ".", key_name_for!($($rest),+))
    }
}

macro_rules! make_getters {
    () => {};
    (#requires_config_location $parent:ident.$field:ident; $($rest:tt)*) => {
        ::paste::paste! {
            #[doc = concat!("Getter for `", key_name_for!($parent,$field), "`")]
            pub fn [< $parent _ $field >](&self) -> config_types::[< $parent:camel $field:camel >] {
                self.stored
                    .$parent()
                    .$field(&self.config_location)
                    .to_owned()
                    .into()
            }
        }
        make_getters!{$($rest)*}
    };

    (#optional $($ident:ident).+ $(, renamed to $renamed:literal)?; $($rest:tt)*) => {
        ::paste::paste! {
            #[doc = concat!("Getter for `", key_name_for!($($ident),+ $(; $renamed)?), "`\n\n# Errors\nThis will return an error ([ConfigSettingError::ConfigNotSet]) iff `", key_name_for!($($ident),+ $(; $renamed)?),"` is not set")]
            $(#[doc(alias = $renamed)])?
            pub fn [< $($ident)_+ >](&self) -> Result<config_types::[< $($ident:camel)+ >], ConfigSettingError> {
                Ok(self.stored
                    $(.$ident())+
                    .ok_or_else(|| ConfigSettingError::ConfigNotSet {
                        key: key_name_for!($($ident),+ $(; $renamed)?),
                    })?
                    .to_owned()
                    .into())
            }
        }
        make_getters!{$($rest)*}
    };

    (#no_getter $($ident:ident).+ with getter $($getter:ident).+ $(, renamed to $renamed:literal)?; $($rest:tt)*) => {
        make_getters!{$($rest)*}
    };

    ($($ident:ident).+ with getter $($getter:ident).+ $(, renamed to $renamed:literal)?; $($rest:tt)*) => {
        ::paste::paste! {
            #[doc = concat!("Getter for `", key_name_for!($($ident),+ $(; $renamed)?), "`")]
            $(#[doc(alias = $renamed)])?
            pub fn [< $($ident)_+ >](&self) -> config_types::[< $($ident:camel)+ >] {
                self.stored
                    $(.$getter())+
                    .to_owned()
                    .into()
            }
        }
        make_getters!{$($rest)*}
    };

    ($($ident:ident).+ $(, renamed to $renamed:literal)?; $($rest:tt)*) => {
        ::paste::paste! {
            #[doc = concat!("Getter for `", key_name_for!($($ident),+ $(; $renamed)?), "`")]
            $(#[doc(alias = $renamed)])?
            pub fn [< $($ident)_+ >](&self) -> config_types::[< $($ident:camel)+ >] {
                self.stored
                    $(.$ident())+
                    .to_owned()
                    .into()
            }
        }
        make_getters!{$($rest)*}
    };

    // Calls the getter for an optional configuration, returning None if the field is None
    (@@string_getter $target:ident #optional $($ident:ident).+) => {
        paste::paste!($target.[< $($ident)_+ >]().ok()?.to_string())
    };

    // Calls the getter for an optional configuration, returning None if the field is None
    (@@string_getter $target:ident $(#$tag:ident)? $($ident:ident).+ ; with getter $($getter:ident).+) => {
        paste::paste!($target.stored.$($getter()).+.to_string())
    };

    // Calls the getter for a configuration with a default value, returning the default
    (@@string_getter $target:ident $(#$tag:ident)? $($ident:ident).+) => {
        paste::paste!($target.[< $($ident)_+ >]().to_string())
    };

    (@@init $( $(#$tag:ident)? $($ident:ident).+ $(with getter $($getter:ident).+)? $(, renamed to $renamed:literal)?;)+) => {
        paste::paste! {
            impl NewTEdgeConfig {
                /// Reads a configuration from the DTO, converting it to a string for use in `tedge config`
                fn read_configured_value_string(&self, key: WritableKey) -> Option<String> {
                    let value = match key {
                        $(
                            WritableKey::[< $($ident:camel)+ >] => make_getters!(@@string_getter self $(#$tag)? $($ident).+ $(; with getter $($getter).+)?),
                        )+
                    };

                    Some(value)
                }

                make_getters! { $( $(#$tag)? $($ident).+ $(with getter $($getter).+)? $(, renamed to $renamed)?;)+ }
            }
        }
    }
}

/// A macro to generate a bunch of configuration accessors
///
/// This generates a few different enums ([ReadableKey], [WritableKey],
/// [ReadOnlyKey], [ConfigurationUpdate])
macro_rules! configuration_keys {
    { $(@readonly $($ro_config_path:ident).+;)* $($(#$tag:ident)? $($config_path:ident).+: $ty:ty $(, field $accessor:literal)? $(, with getter $($getter:ident).+)? $(, renamed to $literal_name:literal)? $(, with default $default:literal)?);+ $(;)? } => {
        make_getters! {
            @@init
            $( $(#$tag)? $($config_path).+ $(with getter $($getter).+)? $(, renamed to $literal_name)?; )+
        }

        paste::paste! {
            #[derive(Copy, Clone, Debug, PartialEq, Eq)]
            /// A configuration setting that can be read using
            /// [NewTEdgeConfig::read]
            pub enum ReadableKey {
                /// A setting that's derived from other configuration
                ReadOnly(ReadOnlyKey),
                /// A setting that is directly user-configurable
                ReadWrite(WritableKey),
            }

            #[derive(Copy, Clone, Debug, PartialEq, Eq)]
            /// A configuration setting that is read only
            #[non_exhaustive]
            pub enum ReadOnlyKey {
                $(
                    #[doc = concat!("`", key_name_for!($($ro_config_path),+), "`")]
                    [< $($ro_config_path:camel)+ >],
                )*
            }

            #[derive(Clone, Debug, PartialEq, Eq)]
            /// A configuration update that ensures the provided value is valid
            ///
            /// This can be used by [update](crate::TEdgeConfigRepository::update).
            #[non_exhaustive]
            pub enum ConfigurationUpdate {
                $(
                    #[doc = concat!("`", key_name_for!($($config_path),+ $(; $literal_name)?), "`")]
                    [< $($config_path:camel)+ >]($ty),
                )+
            }

            #[derive(Debug, Hash, Copy, Clone, PartialEq, Eq)]
            /// A configuration key that can be written to as well as read from
            ///
            /// This can be used by [unset](crate::TEdgeConfigRepository::unset)
            /// to unset a configuration, and
            /// [update_string](crate::TEdgeConfigRepository::update_string) to
            /// set a configuration using a string slice as the value.
            ///
            /// It also implements [FromStr](std::str::FromStr), so it can be
            /// parsed from a string, such as the first argument to `tedge
            /// config set`. This implementation will automatically warn about a
            /// deprecated key if the key has been renamed at some point in the
            /// past.
            ///
            /// ```
            /// use tedge_config::WritableKey;
            /// use tedge_config::ConfigSettingError;
            ///
            /// assert!("c8y.url".parse::<WritableKey>().is_ok());
            ///
            /// // Keys with extra dots in place of `_` are accepted for compatibility with older versions
            /// assert_eq!(
            ///     "mqtt.external_port".parse::<WritableKey>().unwrap(),
            ///     // The below will emit a deprecation warning
            ///     "mqtt.external.port".parse::<WritableKey>().unwrap(),
            /// );
            ///
            /// // Aliases are also accepted
            /// assert_eq!(
            ///     "az.url".parse::<WritableKey>().unwrap(),
            ///     // The below will emit a deprecation warning
            ///     "azure.url".parse::<WritableKey>().unwrap(),
            /// );
            ///
            /// // Specific errors are generated if the key provided is valid but read only
            /// assert!(matches!(
            ///     "device.id".parse::<WritableKey>(),
            ///     Err(ConfigSettingError::WriteToReadOnlySetting { .. })
            /// ));
            /// ```
            #[non_exhaustive]
            pub enum WritableKey {
                $(
                    #[doc = concat!("`", key_name_for!($($config_path),+ $(; $literal_name)?), "`")]
                    [< $($config_path:camel)+ >],
                )+
            }

            impl WritableKey {
                /// Iterates over all the writable keys
                pub fn iter() -> impl Iterator<Item = Self> {
                    [
                        $(
                            Self::[< $($config_path:camel)+ >],
                        )+
                    ].into_iter()
                }
            }

            mod config_types {
                use super::*;

                $(
                    #[allow(unused)]
                    pub type [< $($config_path:camel)+ >] = $ty;
                )+
            }

            #[cfg(test)]
            macro_rules! default_value {
                ($value:literal) => ($value.try_into().unwrap());
                () => (Default::default());
            }

            impl ReadableKey {
                /// Iterates over all the readable keys
                pub fn iter() -> impl Iterator<Item = Self> {
                    [
                        $(
                            Self::ReadOnly(ReadOnlyKey::[< $($ro_config_path:camel)+ >]),
                        )+
                        $(
                            Self::ReadWrite(WritableKey::[< $($config_path:camel)+ >]),
                        )+
                    ].into_iter()
                }

                /// Verifies if the provided key is valid, without normalising first
                fn is_valid(key: &str) -> bool {
                    match key {
                        $(
                            key_name_for!($($ro_config_path),+) => true,
                        )*
                        $(
                            key_name_for!($($config_path),+ $(; $literal_name)?) => true,
                        )+
                        _ => false,
                    }
                }
            }

            #[cfg(test)]
            impl ConfigurationUpdate {
                /// Iterates over all the possible values for [Self], used to generate test data
                fn iter() -> impl Iterator<Item = Self> {
                    [
                        $(
                            Self::[< $($config_path:camel)+ >](default_value!($($default)?)),
                        )+
                    ].into_iter()
                }
            }

            impl ::std::str::FromStr for ReadableKey {
                type Err = ConfigSettingError;

                fn from_str(input: &str) -> Result<Self, Self::Err> {
                    match normalize_key(input).as_str() {
                        $(
                            key_name_for!($($ro_config_path),+) => Ok(Self::ReadOnly(ReadOnlyKey::[< $($ro_config_path:camel)+ >])),
                        )*
                        key => WritableKey::from_str(key).map(Self::ReadWrite),
                    }
                }
            }

            impl From<ReadableKey> for &'static str {
                fn from(key: ReadableKey) -> &'static str {
                    match key {
                        $(
                            ReadableKey::ReadOnly(ReadOnlyKey::[< $($ro_config_path:camel)+ >]) => key_name_for!($($ro_config_path),+),
                        )*
                        ReadableKey::ReadWrite(key) => key.into(),
                    }
                }
            }


            impl From<ReadOnlyKey> for &'static str {
                fn from(key: ReadOnlyKey) -> &'static str {
                    match key {
                        $(
                            ReadOnlyKey::[< $($ro_config_path:camel)+ >] => key_name_for!($($ro_config_path),+),
                        )*
                    }
                }
            }

            const READ_ONLY_KEYS: &[ReadOnlyKey] = &[
                $(ReadOnlyKey::[< $($ro_config_path:camel)+ >],)*
            ];

            impl ::std::str::FromStr for ReadOnlyKey {
                type Err = ConfigSettingError;

                fn from_str(input: &str) -> Result<Self, Self::Err> {
                    match normalize_key(input).as_str() {
                        $(
                            key_name_for!($($ro_config_path),+) => Ok(Self::[< $($ro_config_path:camel)+ >]),
                        )+
                        _ => Err(Self::Err::ReadUnrecognisedKey{
                            key: input.to_owned()
                        }),
                    }
                }
            }

            impl ::std::str::FromStr for WritableKey {
                type Err = ConfigSettingError;

                fn from_str(input: &str) -> Result<Self, Self::Err> {
                    match normalize_key(input).as_str() {
                        $(
                            key_name_for!($($ro_config_path),+) => Err(Self::Err::WriteToReadOnlySetting {
                                message: ReadOnlyKey::[< $($ro_config_path:camel)+ >].read_only_error()
                            }),
                        )+
                        $(
                            key_name_for!($($config_path),+ $(; $literal_name)?) => Ok(Self::[< $($config_path:camel)+ >]),
                        )+
                        _ => Err(Self::Err::WriteUnrecognisedKey{
                            key: input.to_owned()
                        }),
                    }
                }
            }

            impl From<&ConfigurationUpdate> for WritableKey {
                fn from(update: &ConfigurationUpdate) -> WritableKey {
                    match update {
                        $(
                            ConfigurationUpdate::[< $($config_path:camel)+ >](_) => WritableKey::[< $($config_path:camel)+ >],
                        )+
                    }
                }
            }

            impl From<WritableKey> for &'static str {
                fn from(key: WritableKey) -> &'static str {
                    match key {
                        $(
                            WritableKey::[< $($config_path:camel)+ >] => key_name_for!($($config_path),+ $(; $literal_name)?),
                        )+
                    }
                }
            }

            pub fn typed_update(dto: &mut TEdgeConfigDto, update: ConfigurationUpdate) {
                match update {
                    $(
                        ConfigurationUpdate::[< $($config_path:camel)+ >](value) => dto.$($config_path).+ = Some(value $(. $accessor)?.into()),
                    )+
                }
            }

            pub fn typed_unset(dto: &mut TEdgeConfigDto, key: WritableKey) {
                match key {
                    $(
                        WritableKey::[< $($config_path:camel)+ >] => dto.$($config_path).+ = None,
                    )+
                }
            }
        }
    }
}

impl ReadOnlyKey {
    fn ty(&self) -> doku::Type {
        match self {
            Self::DeviceId => {
                let mut ty = String::ty();
                ty.comment = Some("Identifier of the device within the fleet. It must be globally unique and is derived from the device certificate.");
                ty.example = Some(doku::Example::Simple(
                    "Raspberrypi-4d18303a-6d3a-11eb-b1a6-175f6bb72665",
                ));
                ty.metas.add("note", "This setting is derived from the device certificate and therefore is read only.");
                ty
            }
            Self::HttpAddress => {
                let mut ty = IpAddress::ty();
                ty.comment =
                    Some("Http client address, which is used by the File Transfer Service.");
                ty.example = Some(doku::Example::Compound(&["127.0.0.1", "192.168.1.2"]));
                ty
            }
        }
    }
}

/// The keys that can be read from the configuration
pub static READABLE_KEYS: Lazy<Vec<(Cow<'static, str>, doku::Type)>> = Lazy::new(|| {
    let ty = TEdgeConfigDto::ty();
    let doku::TypeKind::Struct { fields, transparent: false } = ty.kind else { panic!("Expected struct but got {:?}", ty.kind) };
    let doku::Fields::Named { fields } = fields else { panic!("Expected named fields but got {:?}", fields)};
    READ_ONLY_KEYS
        .iter()
        .map(|key| (Cow::Borrowed(key.as_str()), key.ty()))
        .chain(struct_field_paths(None, &fields))
        .collect()
});

impl ReadableKey {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ReadOnly(key) => key.as_str(),
            Self::ReadWrite(key) => key.as_str(),
        }
    }
}

impl fmt::Display for ReadableKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl ReadOnlyKey {
    fn as_str(self) -> &'static str {
        self.into()
    }
}

impl fmt::Display for ReadOnlyKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl WritableKey {
    pub fn as_str(self) -> &'static str {
        self.into()
    }
}

impl fmt::Display for WritableKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

configuration_keys! {
    // Readonly keys have to come before other keys
    @readonly device.id;
    @readonly http.address;

    // The defaults for these keys depend on config location, so the getters are
    // generated with that in mind
    #requires_config_location device.key_path: Utf8PathBuf;
    #requires_config_location device.cert_path: Utf8PathBuf;

    // `renamed to` changes the key that is used with `tedge config`, which is
    // otherwise derived from the DTO path.
    // TODO automatically test that this matches the field name according to serde
    device.device_type: String, renamed to "device.type";

    // Optional fields have fallible getters that return an error when the value
    // is not set. An error is returned rather than option so the error message
    // automatically includes the name of the key that is causing the failure.
    #optional c8y.url: ConnectUrl, with default "example.com";
    c8y.root_cert_path: Utf8PathBuf;
    c8y.smartrest_templates: TemplatesSet;

    // Some types, like `ConnectUrl` don't have a default. We need to generate
    // values for testing (ConfigurationUpdate::iter) so we have to supply a
    // dummy default value for any settings that don't have a type that
    // implements Default. The value of this really doesn't matter, the test in
    // question just sets and unsets each setting in turn and check they both
    // modify the same DTO field.
    #optional az.url: ConnectUrl, with default "example.com";
    az.root_cert_path: Utf8PathBuf;
    az.mapper_timestamp: bool;
    #optional aws.url: ConnectUrl, with default "example.com";
    aws.root_cert_path: Utf8PathBuf;
    aws.mapper_timestamp: bool;
    mqtt.bind_address: IpAddress;
    mqtt.port: u16;
    #optional mqtt.external_port: u16;
    #optional mqtt.external_bind_address: IpAddress;
    #optional mqtt.external_bind_interface: String;
    #optional mqtt.external_ca_path: Utf8PathBuf;
    #optional mqtt.external_cert_file: Utf8PathBuf;
    #optional mqtt.external_key_file: Utf8PathBuf;
    mqtt.client_host: String;
    mqtt.client_port: NonZeroU16, with default 8080;
    #optional mqtt.client_auth.cert_file: Utf8PathBuf;
    #optional mqtt.client_auth.key_file: Utf8PathBuf;
    #optional mqtt.client_ca_file: Utf8PathBuf;
    #optional mqtt.client_ca_path: Utf8PathBuf;
    http.port: u16;
    #optional software.default_plugin: String;
    tmp.path: Utf8PathBuf;
    logs.path: Utf8PathBuf;
    run.path: Utf8PathBuf;
    run.lock_files: bool;
    data.path: Utf8PathBuf;

    // This field won't generate a getter, but will have the required
    // logic to be read and written to as a string. This allows tedge
    // config to accept a number of seconds.
    //
    // Consumers of the configuration in thin-edge crates should access
    // this via the manually implemented getter above, which returns
    // a Duration for clarity.
    #no_getter firmware.child_update_timeout: u64, with getter firmware.raw_child_update_timeout;
    service.service_type: String, renamed to "service.type";
}

// TODO test me with testing_logger to check I log exactly once
fn emit_warning_if_necessary(normalized: &str, input: &str) {
    static WARNED_FOR: Lazy<Mutex<HashSet<String>>> = Lazy::new(<_>::default);

    if normalized != input && ReadableKey::is_valid(normalized) {
        let mut previously_warned_for = WARNED_FOR.lock().unwrap();
        if !previously_warned_for.insert(input.to_owned()) {
            tracing::warn!(
                target: "tedge config",
                "The key: '{input}' is deprecated. Use '{normalized}' instead."
            );
        }
    }
}

/// Normalize the provided key to the most up-to-date format
///
/// Previously, keys had many dots, e.g. `mqtt.external.port`, but this mapped to `mqtt.external_port`
/// in the toml, which is confuing. This removes unnecessary dots
fn normalize_key(input: &str) -> String {
    let (prefix, suffix) = input.split_once('.').unwrap_or((input, ""));
    let suffix = suffix.replace('.', "_");
    let normalized = replace_aliases(format!("{prefix}.{suffix}"));
    emit_warning_if_necessary(&normalized, input);
    normalized
}

/// A map from aliases to canonical keys
static ALIASES: Lazy<HashMap<Cow<'static, str>, Cow<'static, str>>> = Lazy::new(|| {
    let ty = TEdgeConfigDto::ty();
    let doku::TypeKind::Struct { fields, transparent: false } = ty.kind else { panic!("Expected struct but got {:?}", ty.kind) };
    let doku::Fields::Named { fields } = fields else { panic!("Expected named fields but got {:?}", fields)};
    struct_field_aliases(None, &fields)
});

fn replace_aliases(key: String) -> String {
    ALIASES
        .get(&Cow::Borrowed(key.as_str()))
        .map(|c| c.clone().into_owned())
        .unwrap_or(key)
}

fn dot_separate(prefix: Option<&str>, field: &str, sub_path: &str) -> Cow<'static, str> {
    Cow::Owned(
        prefix
            .into_iter()
            .chain([field, sub_path])
            .collect::<Vec<_>>()
            .join("."),
    )
}

fn struct_field_aliases(
    prefix: Option<&str>,
    fields: &[(&'static str, doku::Field)],
) -> HashMap<Cow<'static, str>, Cow<'static, str>> {
    fields
        .iter()
        .flat_map(|(field_name, field)| match named_fields(&field.ty.kind) {
            Some(fields) => {
                // e.g. normal_field.alias
                struct_field_aliases(Some(&key_name(prefix, field_name)), fields)
                    .into_iter()
                    // e.g. alias.normal_field
                    .chain(conventional_sub_paths(field, prefix, field_name, fields))
                    // e.g. alias.other_alias
                    .chain(aliased_sub_paths(field, prefix, field_name, fields))
                    .collect::<HashMap<_, _>>()
            }
            None => field
                .aliases
                .iter()
                .map(|alias| (key_name(prefix, alias), key_name(prefix, field_name)))
                .collect(),
        })
        .collect()
}

fn aliased_sub_paths(
    field: &doku::Field,
    prefix: Option<&str>,
    field_name: &str,
    sub_fields: &[(&'static str, doku::Field)],
) -> Vec<(Cow<'static, str>, Cow<'static, str>)> {
    field
        .aliases
        .iter()
        .flat_map(|alias| {
            // e.g. alias.another_alias
            struct_field_aliases(None, sub_fields).into_iter().map(
                move |(nested_alias, resolved_subpath)| {
                    (
                        dot_separate(prefix, alias, &nested_alias),
                        dot_separate(prefix, field_name, &resolved_subpath),
                    )
                },
            )
        })
        .collect()
}

fn conventional_sub_paths(
    field: &doku::Field,
    prefix: Option<&str>,
    name: &str,
    sub_fields: &[(&'static str, doku::Field)],
) -> Vec<(Cow<'static, str>, Cow<'static, str>)> {
    field
        .aliases
        .iter()
        .flat_map(|alias| {
            // e.g. alias.normal_field
            struct_field_paths(None, sub_fields)
                .into_iter()
                .map(move |(path, _ty)| {
                    (
                        dot_separate(prefix, alias, &path),
                        dot_separate(prefix, name, &path),
                    )
                })
        })
        .collect()
}

pub(crate) fn struct_field_paths(
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

fn key_name(prefix: Option<&str>, name: &'static str) -> Cow<'static, str> {
    match prefix {
        Some(prefix) => Cow::Owned(format!("{prefix}.{name}")),
        None => Cow::Borrowed(name),
    }
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

#[cfg(test)]
#[allow(clippy::expect_fun_call)]
mod tests {
    use fake::Dummy;
    use fake::Faker;
    use figment::Figment;

    use super::*;

    #[test]
    fn aliased_field_names_do_not_contain_dots() {
        for (alias, key) in ALIASES.iter() {
            if key.chars().filter(|&c| c == '.').count()
                < alias.chars().filter(|&c| c == '.').count()
            {
                panic!(
                    "Alias contains a dot!\n\n\t\
                     Alias {alias} for {key} is invalid.\n\t\
                     One or more #[serde(alias)] values contains a `.`.\n\t\
                     Hint: this should probably be replaced with `_`.\n\n"
                )
            }
        }
    }

    #[test]
    fn writing_to_a_value_then_reading_it_back_as_a_string_always_succeeds() {
        for (key, value) in example_data() {
            let mut config = default_tedge_config(TEdgeConfigDto::default());

            config.stored = TEdgeConfigUpdate::new(key, &value)
                .apply_to(&config.stored)
                .unwrap();
            assert_eq!(
                config
                    .read_configured_value_string(key),
                Some(value),
                "verifying that read_configured_value_string reading {:?} returns the value written to that key.\
                It's probably worth checking the getter in tedge_config_dto.rs is returning the right thing.", key.as_str(),
            );
        }
    }

    fn writable_keys() -> Vec<(Cow<'static, str>, doku::Type)> {
        let ty = TEdgeConfigDto::ty();
        let doku::TypeKind::Struct { fields, transparent: false } = ty.kind else { panic!("Expected struct but got {:?}", ty.kind) };
        let doku::Fields::Named { fields } = fields else { panic!("Expected named fields but got {:?}", fields)};
        struct_field_paths(None, &fields)
    }

    #[test]
    fn typed_unset_inverts_typed_update() {
        for update in ConfigurationUpdate::iter() {
            let mut dto = TEdgeConfigDto::default();
            let field = WritableKey::from(&update);
            typed_update(&mut dto, update);
            assert_ne!(
                dto,
                TEdgeConfigDto::default(),
                "Update for {field:?} did not change TEdgeConfigDto"
            );
            typed_unset(&mut dto, field);
            assert_eq!(
                dto,
                TEdgeConfigDto::default(),
                "Unset for {field:?} did not reset TEdgeConfigDto"
            );
        }
    }

    #[rstest::rstest]
    #[case("device.type")]
    #[case("device.key.path")]
    #[case("device.cert.path")]
    #[case("c8y.url")]
    #[case("c8y.root.cert.path")]
    #[case("c8y.smartrest.templates")]
    #[case("azure.url")]
    #[case("az.url")]
    #[case("azure.root.cert.path")]
    #[case("az.root.cert.path")]
    #[case("aws.url")]
    #[case("aws.root.cert.path")]
    #[case("aws.mapper.timestamp")]
    #[case("az.mapper.timestamp")]
    #[case("mqtt.bind_address")]
    #[case("mqtt.client.host")]
    #[case("mqtt.client.port")]
    #[case("mqtt.client.ca_file")]
    #[case("mqtt.client.ca_path")]
    #[case("mqtt.port")]
    #[case("http.port")]
    #[case("mqtt.external.port")]
    #[case("mqtt.external.bind_address")]
    #[case("mqtt.external.bind_interface")]
    #[case("mqtt.external.capath")]
    #[case("mqtt.external.ca_path")]
    #[case("mqtt.external.certfile")]
    #[case("mqtt.external.cert_file")]
    #[case("mqtt.external.keyfile")]
    #[case("mqtt.external.key_file")]
    #[case("software.plugin.default")]
    #[case("tmp.path")]
    #[case("logs.path")]
    #[case("run.path")]
    #[case("data.path")]
    #[case("firmware.child.update.timeout")]
    #[case("service.type")]
    #[case("run.lock_files")]
    fn dotted_configuration_keys_map_to_valid_configuration_unsets(#[case] key: &str) {
        key.parse::<WritableKey>().expect(&format!(
            "{key} wasn't handled in `{}::WritableKey::from_str`",
            module_path!().rsplit_once("::").unwrap().0
        ));
    }

    #[rstest::rstest]
    #[case("device.key.path", "/etc/tedge/certs/key.pem")]
    #[case("device.cert.path", "/etc/tedge/certs/cert.pem")]
    #[case("mqtt.external.capath", "/etc/tedge/ca/cert.pem")]
    #[case("mqtt.external.ca_path", "/etc/tedge/ca/cert.pem")]
    #[case("c8y.smartrest.templates", "test,me")]
    #[case("mqtt.external.port", "1883")]
    fn dotted_configuration_keys_map_to_valid_configuration_updates(
        #[case] key: WritableKey,
        #[case] value: &str,
    ) {
        let dto = TEdgeConfigDto::default();
        let res = TEdgeConfigUpdate::new(key, value).apply_to(&dto);
        assert!(
            res.is_ok(),
            "{key} wasn't handled in `{}::apply_to`",
            std::any::type_name::<TEdgeConfigUpdate>()
                .rsplit_once("::")
                .unwrap()
                .0
        );
    }

    #[rstest::rstest]
    #[case("device.key.path", "/etc/tedge/certs/key.pem")]
    #[case("device.cert.path", "/etc/tedge/certs/cert.pem")]
    #[case("mqtt.external.capath", "/etc/tedge/ca/cert.pem")]
    #[case("mqtt.external.ca_path", "/etc/tedge/ca/cert.pem")]
    #[case("mqtt.external.port", "1883")]
    fn a_configuration_value_can_be_written_to_and_read_back(
        #[case] key: WritableKey,
        #[case] value: &str,
    ) {
        let dto = TEdgeConfigDto::default();
        let read_value =
            &default_tedge_config(TEdgeConfigUpdate::new(key, value).apply_to(&dto).unwrap())
                .read(key.into())
                .unwrap()
                .unwrap();
        assert_eq!(read_value, value);
    }

    #[rstest::rstest]
    #[case::device_id("device.id")]
    #[case::http_address("http.address")]
    fn writes_to_read_only_keys_are_rejected(#[case] key: &str) {
        let error = key.parse::<WritableKey>().unwrap_err();
        assert!(matches!(
            error,
            ConfigSettingError::WriteToReadOnlySetting { .. }
        ))
    }

    #[test]
    fn smartrest_templates_are_debug_formatted_when_read() {
        let key = WritableKey::C8ySmartrestTemplates;
        let dto = TEdgeConfigDto::default();
        let read_value = &default_tedge_config(
            TEdgeConfigUpdate::new(key, "templateId1,templateId2")
                .apply_to(&dto)
                .unwrap(),
        )
        .read(key.into())
        .unwrap()
        .unwrap();
        assert_eq!(read_value, r#"["templateId1", "templateId2"]"#);
    }

    #[test]
    fn device_id_can_be_read_from_key() {
        let config = default_tedge_config(TEdgeConfigDto::default());

        config.device_id.set("DEVICE_ID".into()).unwrap();

        assert_eq!(
            config.read("device.id".parse().unwrap()).unwrap().unwrap(),
            "DEVICE_ID"
        );
    }

    #[test]
    fn http_address_can_be_read_from_key() {
        let config = default_tedge_config(TEdgeConfigDto::default());

        assert_eq!(
            config
                .read("http.address".parse().unwrap())
                .unwrap()
                .unwrap(),
            config.http_address().to_string()
        )
    }

    fn default_tedge_config(dto: TEdgeConfigDto) -> NewTEdgeConfig {
        NewTEdgeConfig {
            stored: dto,
            device_id: <_>::default(),
            config_location: <_>::default(),
        }
    }

    mod all_the_keys_in_the_documentation {
        use std::str::FromStr;

        use super::*;

        #[test]
        fn represent_a_valid_configuration() {
            for (key, _) in writable_keys() {
                assert!(
                    WritableKey::from_str(&key).is_ok(),
                    "{key} wasn't defined in call to `{}::configuration_keys!`",
                    module_path!().rsplit_once("::").unwrap().0
                )
            }
        }

        #[test]
        fn represent_a_unique_configurations() {
            let mut keys_for_configurations = HashMap::new();
            for (key, _) in writable_keys() {
                if let Ok(configuration) = WritableKey::from_str(&key) {
                    if let Some(duplicate_key) =
                        keys_for_configurations.insert(configuration, key.clone())
                    {
                        panic!("{duplicate_key} and {key} both map to {configuration:?} in {}::WritableKey", module_path!())
                    }
                }
            }
        }
    }

    #[test]
    fn values_can_be_deserialized_from_custom_figment_provider() {
        let provider = TEdgeConfigUpdate::new(WritableKey::DeviceKeyPath, "/tmp/test");

        let value: TEdgeConfigDto = Figment::new().merge(provider).extract().unwrap();
        assert_eq!(value.device.key_path, Some("/tmp/test".into()));
    }

    mod an_empty_configuration_value {
        use super::*;

        #[test]
        fn is_populated_when_the_value_is_updated() {
            let original = TEdgeConfigDto::default();

            let updated = TEdgeConfigUpdate::new(WritableKey::C8yUrl, "test.cumulocity.com")
                .apply_to(&original)
                .unwrap();

            assert_eq!(
                updated.c8y.url.as_ref().unwrap().as_str(),
                "test.cumulocity.com"
            );
        }

        #[test]
        fn remains_unpopulated_when_the_value_is_removed() {
            let mut dto = TEdgeConfigDto::default();

            typed_unset(&mut dto, WritableKey::DeviceDeviceType);

            assert_eq!(
                toml::to_string(&dto).unwrap(),
                toml::to_string(&TEdgeConfigDto::default()).unwrap()
            );
        }
    }

    mod a_configured_value {
        use super::*;

        #[test]
        fn is_preserved_when_a_different_value_is_updated() {
            let mut original = TEdgeConfigDto::default();
            original.device.device_type = Some("type".into());

            let updated = TEdgeConfigUpdate::new(WritableKey::C8yUrl, "test.cumulocity.com")
                .apply_to(&original)
                .unwrap();

            assert_eq!(updated.device.device_type.as_ref().unwrap(), "type");
        }

        #[test]
        fn is_overwritten_when_a_the_same_value_is_updated() {
            let mut original = TEdgeConfigDto::default();
            original.device.device_type = Some("type".into());

            let updated = TEdgeConfigUpdate::new(WritableKey::DeviceDeviceType, "updated")
                .apply_to(&original)
                .unwrap();

            assert_eq!(updated.device.device_type.as_ref().unwrap(), "updated");
        }

        #[test]
        fn is_removed_when_the_value_is_unset() {
            let mut dto = TEdgeConfigDto::default();
            dto.device.device_type = Some("value".into());

            typed_unset(&mut dto, WritableKey::DeviceDeviceType);

            assert_eq!(dto.device.device_type, None);
        }
    }

    mod an_unrecognised_configuration_key {
        use super::*;

        #[test]
        fn is_rejected_when_attempting_to_update_or_remove_the_value() {
            let ConfigSettingError::WriteUnrecognisedKey { key } = "unrecognised.key"
                .parse::<WritableKey>()
                .unwrap_err() else { panic!() };

            assert_eq!(key, "unrecognised.key");
        }
    }

    mod an_invalid_configuration_value {
        use super::*;

        #[test]
        fn is_rejected_when_updating_the_value() {
            let original = TEdgeConfigDto::default();

            let ConfigSettingError::Figment { error, key } =
                TEdgeConfigUpdate::new(WritableKey::HttpPort, "not a port")
                    .apply_to(&original)
                    .unwrap_err() else { panic!() };

            assert_eq!(key, WritableKey::HttpPort);
            assert!(error.to_string().contains("not a port"))
        }
    }

    mod the_alias_map {
        use super::*;
        mod for_a_simple_struct {
            use super::*;

            #[test]
            fn is_empty_when_no_aliases_are_used() {
                #[derive(serde::Deserialize, Document)]
                #[allow(unused)]
                struct AliasFree {
                    name: String,
                }

                let expected = HashMap::new();

                assert_eq!(struct_field_aliases(None, &fields::<AliasFree>()), expected);
            }

            #[test]
            fn connects_an_alias_to_its_original_field() {
                #[derive(serde::Deserialize, Document)]
                #[allow(unused)]
                struct SingleAlias {
                    #[serde(alias = "alias")]
                    name: String,
                }

                let mut expected = HashMap::new();
                expected.insert(Cow::Borrowed("alias"), Cow::Borrowed("name"));

                assert_eq!(
                    struct_field_aliases(None, &fields::<SingleAlias>()),
                    expected
                );
            }
        }

        mod for_nested_structs {
            use super::*;

            #[test]
            fn contains_the_outer_field_name_when_it_is_not_aliased() {
                #[derive(serde::Deserialize, Document)]
                #[allow(unused)]
                struct AliasContainer {
                    inner: SingleAlias,
                }

                #[derive(serde::Deserialize, Document)]
                #[allow(unused)]
                struct SingleAlias {
                    #[serde(alias = "alias")]
                    name: String,
                }

                let mut expected = HashMap::new();
                expected.insert(Cow::Borrowed("inner.alias"), Cow::Borrowed("inner.name"));

                assert_eq!(
                    struct_field_aliases(None, &fields::<AliasContainer>()),
                    expected
                );
            }

            #[test]
            fn contains_the_outer_alias_when_one_is_used() {
                #[derive(serde::Deserialize, Document)]
                #[allow(unused)]
                struct OuterAlias {
                    #[serde(alias = "alias")]
                    inner: Inner,
                }

                #[derive(serde::Deserialize, Document)]
                #[allow(unused)]
                struct Inner {
                    name: String,
                    second: i32,
                }

                let mut expected = HashMap::new();
                expected.insert(Cow::Borrowed("alias.name"), Cow::Borrowed("inner.name"));
                expected.insert(Cow::Borrowed("alias.second"), Cow::Borrowed("inner.second"));

                assert_eq!(
                    struct_field_aliases(None, &fields::<OuterAlias>()),
                    expected
                );
            }

            #[test]
            fn contains_all_combinations_of_an_aliased_outer_field_name_and_aliased_subfields() {
                #[derive(serde::Deserialize, Document)]
                #[allow(unused)]
                struct NestedAlias {
                    #[serde(alias = "alias")]
                    inner: SingleAlias,
                }

                #[derive(serde::Deserialize, Document)]
                #[allow(unused)]
                struct SingleAlias {
                    #[serde(alias = "also_aliased")]
                    name: String,
                }

                let mut expected = HashMap::new();
                expected.insert(
                    Cow::Borrowed("alias.also_aliased"),
                    Cow::Borrowed("inner.name"),
                );
                expected.insert(Cow::Borrowed("alias.name"), Cow::Borrowed("inner.name"));
                expected.insert(
                    Cow::Borrowed("inner.also_aliased"),
                    Cow::Borrowed("inner.name"),
                );

                assert_eq!(
                    struct_field_aliases(None, &fields::<NestedAlias>()),
                    expected
                );
            }
        }
    }

    fn fields<T: Document>() -> Vec<(&'static str, doku::Field)> {
        match T::ty().kind {
            doku::TypeKind::Struct {
                fields: doku::Fields::Named { fields },
                ..
            } => fields,
            _ => panic!(
                "{} is not a struct with named fields",
                std::any::type_name::<T>()
            ),
        }
    }

    fn example_data() -> impl Iterator<Item = (WritableKey, String)> {
        let mut dummy_configuration = default_tedge_config(TEdgeConfigDto::dummy(&Faker));
        let config = default_tedge_config(TEdgeConfigDto::default());

        WritableKey::iter().map(move |key| {
            let mut count_fails = 0;
            let mut count_failure = || {
                count_fails += 1;
                if count_fails > 30 {
                    panic!(
                        "Getter for {:?} is always returning a default value",
                        key.as_str()
                    )
                }
            };

            // [fake] generates None sometimes for Options which is annoying in
            // our case. So if the value is None, regenerate the data until that
            // isn't the case
            let example_value = loop {
                let value = dummy_configuration.read_configured_value_string(key);
                match value {
                    value if value == config.read_configured_value_string(key) => {
                        // If the value has a default, it will match the value
                        // in the default config, we don't want this
                        count_failure();
                    }
                    None => {
                        // If the value is always returning None, reject it
                        count_failure();
                    }
                    Some(value) => break value,
                }
                dummy_configuration.stored = TEdgeConfigDto::dummy(&Faker);
            };

            (key, example_value)
        })
    }
}
