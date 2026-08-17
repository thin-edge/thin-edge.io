//! Exercises editing a stored config: each write touches the targeted key
//! alone, and a rejected value leaves nothing behind.

use tedge_config_engine::*;

tedge_config_engine_macro::define_config! {
    Test {
        mqtt: {
            port: u16,
            host: String,

            client: {
                id: String,
                port: u16,
            },
        },
        device: {
            id: String,
        },
    }
}

#[test]
fn setting_a_key_leaves_its_siblings_untouched() {
    let mgr = manager();
    let mut dto = TestConfigDto::default();
    mgr.set(&mut dto, "mqtt.host", "example.com").unwrap();
    mgr.set(&mut dto, "device.id", "test-device").unwrap();

    mgr.set(&mut dto, "mqtt.port", "8883").unwrap();

    assert_eq!(
        mgr.get(&dto, "mqtt.host").unwrap().as_deref(),
        Some("example.com")
    );
    assert_eq!(
        mgr.get(&dto, "device.id").unwrap().as_deref(),
        Some("test-device")
    );
    assert_eq!(mgr.get(&dto, "mqtt.port").unwrap().as_deref(), Some("8883"));
}

#[test]
fn setting_a_key_in_an_unset_group_creates_the_group() {
    let mgr = manager();
    let mut dto = TestConfigDto::default();

    mgr.set(&mut dto, "mqtt.client.id", "tedge").unwrap();

    assert_eq!(
        mgr.get(&dto, "mqtt.client.id").unwrap().as_deref(),
        Some("tedge")
    );
}

#[test]
fn unsetting_a_key_leaves_its_siblings_untouched() {
    let mgr = manager();
    let mut dto = TestConfigDto::default();
    mgr.set(&mut dto, "mqtt.port", "8883").unwrap();
    mgr.set(&mut dto, "mqtt.host", "example.com").unwrap();

    mgr.unset(&mut dto, "mqtt.port").unwrap();

    assert_eq!(mgr.get(&dto, "mqtt.port").unwrap(), None);
    assert_eq!(
        mgr.get(&dto, "mqtt.host").unwrap().as_deref(),
        Some("example.com")
    );
}

#[test]
fn a_rejected_value_leaves_the_config_as_it_was() {
    let mgr = manager();
    let mut dto = TestConfigDto::default();
    mgr.set(&mut dto, "mqtt.port", "8883").unwrap();

    let err = mgr.set(&mut dto, "mqtt.port", "not-a-port").unwrap_err();

    assert_eq!(
        err.to_string(),
        "Failed to parse value: failed to parse \"not-a-port\" as u16"
    );
    assert_eq!(mgr.get(&dto, "mqtt.port").unwrap().as_deref(), Some("8883"));
}

#[test]
fn a_rejected_value_does_not_create_the_group_it_was_destined_for() {
    let mgr = manager();
    let mut dto = TestConfigDto::default();

    mgr.set(&mut dto, "mqtt.client.port", "not-a-port")
        .unwrap_err();

    assert_eq!(facet_toml::to_string(&dto).unwrap(), "");
}

fn manager() -> ConfigManager {
    ConfigManager::from_schema::<TestConfig>(std::path::Path::new("/etc/tedge"))
}
