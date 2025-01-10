use camino::Utf8Path;
use camino::Utf8PathBuf;
use certificate::CertificateError;
use certificate::PemCertificate;
use std::borrow::Cow;
use tedge_config_macros::*;

#[derive(thiserror::Error, Debug)]
pub enum ReadError {
    #[error(transparent)]
    ConfigNotSet(#[from] ConfigNotSet),

    #[error(transparent)]
    Multi(#[from] MultiError),

    #[error("Config value {key}, cannot be read: {message} ")]
    ReadOnlyNotFound {
        key: Cow<'static, str>,
        message: &'static str,
    },

    #[error("Derivation for `{key}` failed: {cause}")]
    DerivationFailed {
        key: Cow<'static, str>,
        cause: String,
    },
}

pub trait AppendRemoveItem {
    type Item;

    fn append(current_value: Option<Self::Item>, new_value: Self::Item) -> Option<Self::Item>;

    fn remove(current_value: Option<Self::Item>, remove_value: Self::Item) -> Option<Self::Item>;
}

impl<T> AppendRemoveItem for T {
    type Item = T;

    fn append(_current_value: Option<Self::Item>, _new_value: Self::Item) -> Option<Self::Item> {
        unimplemented!()
    }

    fn remove(_current_value: Option<Self::Item>, _remove_value: Self::Item) -> Option<Self::Item> {
        unimplemented!()
    }
}

define_tedge_config! {
    device: {
        #[tedge_config(reader(function = "device_id"))]
        #[doku(as = "String")]
        id: Result<String, ReadError>,

        #[doku(as = "String")]
        cert_path: Utf8PathBuf,
    },

    #[tedge_config(multi)]
    c8y: {
        device: {
            #[tedge_config(reader(function = "c8y_device_id"))]
            #[tedge_config(default(from_optional_key = "device.id"))]
            #[doku(as = "String")]
            id: Result<String, ReadError>,

            #[doku(as = "String")]
            #[tedge_config(default(from_optional_key = "device.cert_path"))]
            cert_path: Utf8PathBuf,
        }
    },
}

fn device_id(
    device: &TEdgeConfigReaderDevice,
    dto_value: &OptionalConfig<String>,
) -> Result<String, ReadError> {
    match dto_value.or_none() {
        Some(dto_value) => Ok(dto_value.to_owned()),
        None => {
            let cert = device.cert_path.or_config_not_set()?;
            device_id_from_cert(cert)
        }
    }
}

fn c8y_device_id(
    c8y_device: &TEdgeConfigReaderC8yDevice,
    dto_value: &OptionalConfig<String>,
) -> Result<String, ReadError> {
    match dto_value.or_none() {
        Some(dto_value) => Ok(dto_value.to_owned()),
        None => {
            let cert = c8y_device.cert_path.or_config_not_set()?;
            device_id_from_cert(cert)
        }
    }
}

fn device_id_from_cert(cert_path: &Utf8Path) -> Result<String, ReadError> {
    let pem = PemCertificate::from_pem_file(cert_path)
        .map_err(|err| cert_error_into_config_error(ReadableKey::DeviceId.to_cow_str(), err))?;
    let device_id = pem
        .subject_common_name()
        .map_err(|err| cert_error_into_config_error(ReadableKey::DeviceId.to_cow_str(), err))?;
    Ok(device_id)
}

fn cert_error_into_config_error(key: Cow<'static, str>, err: CertificateError) -> ReadError {
    match &err {
        CertificateError::IoError { error, .. } => match error.kind() {
            std::io::ErrorKind::NotFound => ReadError::ReadOnlyNotFound {
                key,
                message: concat!(
                    "The device id is read from the device certificate.\n",
                    "To set 'device.id' to some <id>, you can use `tedge cert create --device-id <id>`.",
                ),
            },
            _ => ReadError::DerivationFailed {
                key,
                cause: format!("{}", err),
            },
        },
        _ => ReadError::DerivationFailed {
            key,
            cause: format!("{}", err),
        },
    }
}

fn read_config(toml: &str) -> TEdgeConfigReader {
    let c8y_dto = toml::from_str(toml).unwrap();
    TEdgeConfigReader::from_dto(&c8y_dto, &TEdgeConfigLocation)
}

fn main() {
    let config = read_config("device.id = \"test-device-id\"");
    let c8y = config.c8y.try_get::<&str>(None).unwrap();

    assert_eq!(config.device.id().unwrap(), "test-device-id");
    assert_eq!(c8y.device.id().unwrap(), "test-device-id");

    let config = read_config("device.id = \"test-device-id\"\nc8y.device.id = \"c8y-device-id\"");
    let c8y = config.c8y.try_get::<&str>(None).unwrap();

    assert_eq!(config.device.id().unwrap(), "test-device-id");
    assert_eq!(c8y.device.id().unwrap(), "c8y-device-id");
}
