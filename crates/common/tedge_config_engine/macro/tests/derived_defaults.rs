//! Exercises `default(from_key_via(...))` through a test-specific config,
//! covering derivation into a non-string field type

use tedge_config_engine::*;

tedge_config_engine_macro::define_config! {
    Derived {
        mqtt: {
            /// MQTT broker port
            #[tedge_config(default(value = "1883"))]
            port: u16,

            /// TLS port derived from the plain MQTT port
            #[tedge_config(default(from_key_via(key = "mqtt.port", function = "next_port")))]
            tls_port: u16,
        },

        device: {
            /// Marker value that makes derivation fail
            id: String,

            /// Uppercased device identifier
            #[tedge_config(default(from_key_via(key = "device.id", function = "shouted_id")))]
            shouted_id: String,
        },
    }
}

#[test]
fn u16_field_derives_from_the_source_key_default() {
    let mgr = manager();
    let dto = DerivedConfigDto::default();
    assert_eq!(
        mgr.read(&dto, "mqtt.tls_port").unwrap(),
        Some("1884".into())
    );
}

#[test]
fn u16_field_follows_an_explicitly_set_source_key() {
    let mgr = manager();
    let mut dto = DerivedConfigDto::default();
    mgr.set(&mut dto, "mqtt.port", "9000").unwrap();
    assert_eq!(
        mgr.read(&dto, "mqtt.tls_port").unwrap(),
        Some("9001".into())
    );
}

#[test]
fn explicitly_set_value_wins_over_the_derived_default() {
    let mgr = manager();
    let mut dto = DerivedConfigDto::default();
    mgr.set(&mut dto, "mqtt.tls_port", "8884").unwrap();
    assert_eq!(
        mgr.read(&dto, "mqtt.tls_port").unwrap(),
        Some("8884".into())
    );
}

#[test]
fn reader_parses_the_derived_value_into_the_field_type() {
    let mgr = manager();
    let dto = DerivedConfigDto::default();
    let config: DerivedConfig = mgr.build_reader(&dto, None, "", None).unwrap();
    assert_eq!(config.mqtt.tls_port.or_none(), Some(&1884u16));
}

#[test]
fn derived_key_is_unset_when_the_source_key_is_unset() {
    let mgr = manager();
    let dto = DerivedConfigDto::default();
    assert_eq!(mgr.read(&dto, "device.shouted_id").unwrap(), None);
}

#[test]
fn derivation_failure_names_the_key_source_and_reason() {
    let mgr = manager();
    let mut dto = DerivedConfigDto::default();
    mgr.set(&mut dto, "device.id", "unpronounceable").unwrap();
    let err = mgr.read(&dto, "device.shouted_id").unwrap_err();
    assert_eq!(
        err.to_string(),
        "Failed to derive a value for 'device.shouted_id' from device.id 'unpronounceable': cannot shout that"
    );
}

fn manager() -> ConfigManager {
    ConfigManager::from_schema::<DerivedConfig>(std::path::Path::new("/etc/tedge"))
}

fn next_port(port: &str) -> Result<Option<u16>, String> {
    let port: u16 = port.parse().map_err(|e| format!("not a port: {e}"))?;
    Ok(Some(port + 1))
}

fn shouted_id(id: &str) -> Result<Option<String>, String> {
    if id == "unpronounceable" {
        return Err("cannot shout that".into());
    }
    Ok(Some(id.to_uppercase()))
}
