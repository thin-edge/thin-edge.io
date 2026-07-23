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
    let serialized = serde_json::to_value(&dto).unwrap();
    let mqtt = &serialized["mqtt"];

    assert_eq!(mqtt["port"], 8883, "set field should be present");
    assert!(
        mqtt.get("host").is_none(),
        "unset field 'host' should not appear in serialized output, got: {mqtt}"
    );
    assert!(
        serialized.get("device").is_none(),
        "entirely unset group 'device' should not appear in serialized output, got: {serialized}"
    );
}

#[test]
fn set_values_survive_serialization_and_deserialization() {
    let mgr = manager();
    let mut dto = TestConfigDto::default();
    mgr.set(&mut dto, "mqtt.port", "8883").unwrap();
    mgr.set(&mut dto, "device.id", "test-device").unwrap();

    let serialized = toml::to_string(&dto).unwrap();
    let deserialized: TestConfigDto = toml::from_str(&serialized).unwrap();

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
