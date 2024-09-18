use camino::Utf8PathBuf;
// use std::net::IpAddr;
// use std::net::Ipv4Addr;
use std::num::NonZeroU16;
// use std::path::PathBuf;
use tedge_config_macros::*;

static DEFAULT_ROOT_CERT_PATH: &str = "/etc/ssl/certs";

#[derive(thiserror::Error, Debug)]
pub enum ReadError {
    #[error(transparent)]
    ConfigNotSet(#[from] ConfigNotSet),
    #[error("Something went wrong: {0}")]
    GenericError(String),
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
    #[tedge_config(multi)]
    c8y: {
        #[tedge_config(reader(private))]
        url: String,
    },
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
    // let dto = TEdgeConfigDto::default();
    // dto.mqtt.bind.address = Some(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)));

    // let config = TEdgeConfigReader::from_dto(&dto, &TEdgeConfigLocation);

    // Typed reads
    // println!(
    //     "Device id is {}.",
    //     // We have to pass the config into try_read to avoid TEdgeConfigReader being
    //     // self-referential
    //     config.device.id.try_read(&config).as_ref().unwrap()
    // );
    // assert_eq!(u16::from(config.mqtt.bind.port), 1883);
    // assert_eq!(config.mqtt.external.bind.port.or_none(), None);
    // assert_eq!(
    //     config.read_string(ReadableKey::DeviceId).unwrap(),
    //     "dummy-device-id"
    // );
}
