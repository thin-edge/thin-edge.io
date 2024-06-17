use tedge_config_macros::*;

#[derive(thiserror::Error, Debug)]
pub enum ReadError {
    #[error(transparent)]
    ConfigNotSet(#[from] ConfigNotSet),
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
    #[tedge_config(deprecated_name = "azure")]
    az: {
        mapper: {
            timestamp: bool,
        }
    },
    device: {
        #[tedge_config(rename = "type")]
        ty: bool,
    }
}

#[test]
fn aliases_can_be_parsed_to_writable_keys() {
    let _: WritableKey = "az.mapper.timestamp".parse().unwrap();
    let _: WritableKey = "azure.mapper.timestamp".parse().unwrap();
}

#[test]
fn aliases_can_be_parsed_to_readable_keys() {
    let _: ReadableKey = "az.mapper.timestamp".parse().unwrap();
    let _: ReadableKey = "azure.mapper.timestamp".parse().unwrap();
}

#[test]
fn renamed_fields_can_be_parsed_to_writable_keys() {
    let _: WritableKey = "device.type".parse().unwrap();
}
