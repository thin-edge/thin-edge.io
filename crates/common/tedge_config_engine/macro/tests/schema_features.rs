//! Exercises macro features the production schemas don't use, each through
//! a test-specific config: zero-arg function defaults, `from_optional_key`,
//! derivation from a renamed key, cyclic defaults, and registry validation

mod features {
    use super::*;
    use tedge_config_engine::*;

    tedge_config_engine_macro::define_config! {
        Features {
            c8y: {
                /// Cloud URL
                url: String,

                /// HTTP endpoint, defaulting to the cloud URL
                #[tedge_config(default(from_optional_key = "c8y.url"))]
                http: String,
            },

            device: {
                /// Device type identifier
                #[tedge_config(rename = "type", default(value = "thin-edge.io"))]
                ty: String,

                /// Device type, loudly
                #[tedge_config(default(from_key_via(key = "device.type", function = "shouted")))]
                loud_type: String,
            },

            run: {
                /// Freshly generated identifier
                #[tedge_config(default(function = "generated_value"))]
                stamp: String,
            },

            cycle: {
                /// One half of a defaulting cycle
                #[tedge_config(default(from_optional_key = "cycle.b"))]
                a: String,

                /// The other half of a defaulting cycle
                #[tedge_config(default(from_optional_key = "cycle.a"))]
                b: String,
            },
        }
    }

    pub fn manager() -> ConfigManager {
        ConfigManager::from_schema::<FeaturesConfig>(std::path::Path::new("/etc/tedge"))
    }

    #[test]
    fn function_default_is_used_when_the_key_is_unset() {
        let mgr = manager();
        let dto = FeaturesConfigDto::default();
        assert_eq!(
            mgr.read(&dto, "run.stamp").unwrap(),
            Some("generated".into())
        );
    }

    #[test]
    fn explicitly_set_value_wins_over_a_function_default() {
        let mgr = manager();
        let mut dto = FeaturesConfigDto::default();
        mgr.set(&mut dto, "run.stamp", "pinned").unwrap();
        assert_eq!(mgr.read(&dto, "run.stamp").unwrap(), Some("pinned".into()));
    }

    #[test]
    fn optional_key_default_follows_the_source_when_set() {
        let mgr = manager();
        let mut dto = FeaturesConfigDto::default();
        mgr.set(&mut dto, "c8y.url", "example.com").unwrap();
        assert_eq!(
            mgr.read(&dto, "c8y.http").unwrap(),
            Some("example.com".into())
        );
    }

    #[test]
    fn optional_key_default_stays_unset_with_its_source() {
        let mgr = manager();
        let dto = FeaturesConfigDto::default();
        assert_eq!(mgr.read(&dto, "c8y.http").unwrap(), None);
    }

    #[test]
    fn unset_optional_key_reader_field_names_the_source_key() {
        let mgr = manager();
        let dto = FeaturesConfigDto::default();
        let config: FeaturesConfig = mgr.build_reader(&dto, None, "", None).unwrap();
        assert_eq!(config.c8y.http.key(), "c8y.url");
    }

    #[test]
    fn derived_default_follows_a_renamed_source_key() {
        let mgr = manager();
        let dto = FeaturesConfigDto::default();
        assert_eq!(
            mgr.read(&dto, "device.loud_type").unwrap(),
            Some("THIN-EDGE.IO".into())
        );
    }

    #[test]
    fn cyclic_optional_defaults_error_instead_of_looping() {
        let mgr = manager();
        let dto = FeaturesConfigDto::default();
        let err = mgr.read(&dto, "cycle.a").unwrap_err();
        assert!(
            err.to_string().contains("Cycle detected"),
            "unexpected error: {err}"
        );
    }
}

mod broken {
    use tedge_config_engine::*;

    tedge_config_engine_macro::define_config! {
        Broken {
            device: {
                /// Falls back to a key that has no default and may not be set
                #[tedge_config(default(from_key = "device.name"))]
                id: String,

                /// A key with no default
                name: String,
            },
        }
    }

    #[test]
    #[should_panic(expected = "invalid defaults registry")]
    fn required_fallback_to_a_defaultless_key_is_rejected_at_startup() {
        ConfigManager::from_schema::<BrokenConfig>(std::path::Path::new("/etc/tedge"));
    }
}

fn generated_value() -> String {
    "generated".into()
}

fn shouted(value: &str) -> Result<Option<String>, String> {
    Ok(Some(value.to_uppercase()))
}
