use tedge_config_macros::*;

define_tedge_config! {
    #[serde(alias = "azure")]
    az: {
        mapper: {
            timestamp: bool,
        }
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
