use tedge_config_engine::*;

tedge_config_engine_macro::define_config! {
    Test {
        mqtt: {
            port: u16,
            host: String,
        },
        device: {
            id: String,
        },
    }
}

fn manager() -> ConfigManager {
    ConfigManager::from_schema::<TestConfig>(std::path::Path::new("/etc/tedge"))
}

#[test]
fn unset_leaf_fields_are_omitted_from_serialized_output() {
    let mgr = manager();
    let mut dto = TestConfigDto::default();
    mgr.set(&mut dto, "mqtt.port", "8883").unwrap();

    // JSON serializes None as null unless skip_serializing_if is set,
    // unlike TOML which omits None fields by default.
    let serialized = facet_value::to_value(&dto).unwrap();
    let mqtt = &serialized.as_object().unwrap()["mqtt"];

    assert_eq!(
        mqtt.as_object().unwrap()["port"],
        facet_value::value!(8883),
        "set field should be present"
    );
    assert!(
        !mqtt.as_object().unwrap().contains_key("host"),
        "unset field 'host' should not appear in serialized output, got: {mqtt:?}"
    );
    assert!(
        serialized.as_object().unwrap().get("device").is_none(),
        "entirely unset group 'device' should not appear in serialized output, got: {serialized:?}"
    );
}

#[test]
fn set_values_survive_serialization_and_deserialization() {
    let mgr = manager();
    let mut dto = TestConfigDto::default();
    mgr.set(&mut dto, "mqtt.port", "8883").unwrap();
    mgr.set(&mut dto, "device.id", "test-device").unwrap();

    let serialized = facet_toml::to_string(&dto).unwrap();
    let deserialized: TestConfigDto = facet_toml::from_str(&serialized).unwrap();

    assert_eq!(
        mgr.read(&deserialized, "mqtt.port").unwrap(),
        Some("8883".into())
    );
    assert_eq!(
        mgr.read(&deserialized, "device.id").unwrap(),
        Some("test-device".into())
    );
    assert_eq!(mgr.read(&deserialized, "mqtt.host").unwrap(), None);
}
