use facet_config_runtime::federated::FederatedConfig;
use facet_config_runtime::ops::{ConfigOps, TypedConfigOps};
use facet_config_runtime::{ConfigManager, EnvOverrides};
use std::path::Path;

mod schemas {
    use facet_config_runtime::*;

    facet_config_macro::define_config! {
        Device {
            /// Uppercased identifier derived from the device name
            #[tedge_config(default(from_key_via(key = "name", function = "shouted")))]
            id: String,

            /// Device name
            #[tedge_config(example = "my-device", deprecated_key = "label")]
            name: String,

            /// Path to the device certificate
            #[tedge_config(default(from_root = "device.cert_path"))]
            cert_path: String,

            /// Path to the device private key, defaulting to the certificate path
            #[tedge_config(default(from_key = "cert_path"))]
            key_path: String,

            /// Negotiated port; reported but not settable
            #[tedge_config(readonly, default(value = "1738"))]
            port: u16,
        }
    }

    facet_config_macro::define_config! {
        Mapper {
            /// Cloud endpoint URL
            url: String,

            /// Identity of the device this mapper connects to the cloud
            device: extern DeviceConfig,
        }
    }

    fn shouted(value: &str) -> Result<Option<String>, String> {
        Ok(Some(value.to_uppercase()))
    }
}

mod root {
    use facet_config_runtime::*;

    facet_config_macro::define_config! {
        Root {
            device: {
                /// Path to the device certificate
                #[tedge_config(default(value = "/etc/tedge/cert.pem"))]
                cert_path: String,
            },
        }
    }

    pub fn source() -> Box<dyn ops::ConfigOps> {
        let manager =
            ConfigManager::from_schema::<RootConfig>(std::path::Path::new("/nonexistent/tedge"));
        super::typed_source::<RootConfigDto>(manager)
    }
}

fn manager() -> ConfigManager {
    ConfigManager::from_schema::<schemas::MapperConfig>(Path::new("/nonexistent/tedge"))
}

mod key_space {
    use super::*;

    #[test]
    fn mounted_keys_are_listed_under_the_mount_key() {
        let mgr = manager();
        let mut keys = mgr.keys::<schemas::MapperConfigDto>();
        keys.sort();
        assert_eq!(
            keys,
            vec![
                "device.cert_path",
                "device.id",
                "device.key_path",
                "device.name",
                "device.port",
                "url",
            ]
        );
    }

    #[test]
    fn mounted_examples_and_docs_appear_on_the_mounted_key() {
        let mgr = manager();
        let entries = mgr.key_entries::<schemas::MapperConfigDto>();
        let name = entries.iter().find(|e| e.key == "device.name").unwrap();
        assert_eq!(name.examples, &["my-device"]);
        assert_eq!(name.doc.first().map(|d| d.trim()), Some("Device name"));
    }

    #[test]
    fn mounted_aliases_are_remapped_on_both_sides() {
        let mgr = manager();
        assert_eq!(
            mgr.resolve_key("device.label"),
            ("device.name".to_owned(), Some("device.label"))
        );
    }

    #[test]
    fn mounted_read_only_keys_are_enforced_at_the_mounted_key() {
        let mgr = manager();
        assert!(mgr.check_read_only("device.port").is_err());
        assert!(mgr.check_read_only("device.name").is_ok());
    }
}

mod defaults {
    use super::*;

    #[test]
    fn value_default_resolves_at_the_mounted_key() {
        let mgr = manager();
        let dto = schemas::MapperConfigDto::default();
        assert_eq!(mgr.read(&dto, "device.port").unwrap(), Some("1738".into()));
    }

    #[test]
    fn relative_from_key_via_source_follows_the_mount() {
        let mgr = manager();
        let mut dto = schemas::MapperConfigDto::default();
        mgr.set(&mut dto, "device.name", "my-device").unwrap();
        assert_eq!(
            mgr.read(&dto, "device.id").unwrap(),
            Some("MY-DEVICE".into())
        );
    }

    #[test]
    fn relative_from_key_via_is_unset_when_its_source_is_unset() {
        let mgr = manager();
        let dto = schemas::MapperConfigDto::default();
        assert_eq!(mgr.read(&dto, "device.id").unwrap(), None);
    }

    #[test]
    fn relative_from_key_source_follows_the_mount() {
        let mgr = manager();
        let mut dto = schemas::MapperConfigDto::default();
        mgr.set(&mut dto, "device.cert_path", "/mapper/cert.pem")
            .unwrap();
        assert_eq!(
            mgr.read(&dto, "device.key_path").unwrap(),
            Some("/mapper/cert.pem".into())
        );
    }

    #[test]
    fn from_root_key_is_not_remapped_by_the_mount() {
        let mgr = manager();
        let dto = schemas::MapperConfigDto::default();
        let resolve = |key: &str| {
            assert_eq!(key, "device.cert_path");
            Ok(Some("/root/cert.pem".into()))
        };
        assert_eq!(
            mgr.read_with_root(&dto, "device.cert_path", Some(&resolve))
                .unwrap(),
            Some("/root/cert.pem".into())
        );
    }
}

mod readers {
    use super::*;

    #[test]
    fn reader_embeds_the_mounted_schemas_reader() {
        let mgr = manager();
        let mut dto = schemas::MapperConfigDto::default();
        mgr.set(&mut dto, "device.name", "my-device").unwrap();
        let resolve = |_: &str| Ok(Some("/root/cert.pem".to_owned()));
        let config: schemas::MapperConfig = mgr.build_reader(&dto, Some(&resolve), "").unwrap();
        assert_eq!(config.device.id.or_none(), Some(&"MY-DEVICE".to_string()));
        assert_eq!(config.device.port, 1738);
        assert_eq!(config.device.key_path, "/root/cert.pem");
        assert_eq!(
            config.device.cert_path.or_none(),
            Some(&"/root/cert.pem".to_string())
        );
    }
}

mod environment {
    use super::*;

    #[test]
    fn env_override_reaches_a_mounted_key() {
        let mgr = manager().with_env_prefix("TEST_");
        let mut dto = schemas::MapperConfigDto::default();
        let env = EnvOverrides::from_pairs(vec![(
            "TEST_DEVICE_NAME".to_owned(),
            "env-device".to_owned(),
        )]);
        let warnings = mgr.apply_env(&mut dto, &env, &[]);
        assert_eq!(warnings, Vec::<String>::new());
        assert_eq!(
            mgr.read(&dto, "device.id").unwrap(),
            Some("ENV-DEVICE".into())
        );
    }
}

mod mounting_into_a_federated_config {
    use super::*;

    #[test]
    fn from_root_inside_a_mounted_schema_resolves_through_the_root_config() {
        let mut fed = federated();
        fed.mount("", root::source()).unwrap();
        fed.mount("mappers.test.", mapper_source()).unwrap();

        assert_eq!(
            fed.read("mappers.test.device.cert_path").unwrap(),
            Some("/etc/tedge/cert.pem".into())
        );
    }

    #[test]
    fn relative_from_key_resolves_through_the_root_fallback() {
        let mut fed = federated();
        fed.mount("", root::source()).unwrap();
        fed.mount("mappers.test.", mapper_source()).unwrap();

        assert_eq!(
            fed.read("mappers.test.device.key_path").unwrap(),
            Some("/etc/tedge/cert.pem".into())
        );
    }

    #[test]
    fn mount_before_the_root_config_names_the_mounted_key() {
        let mut fed = federated();

        let err = fed.mount("mappers.test.", mapper_source()).unwrap_err();

        assert_eq!(
            err.to_string(),
            "'mappers.test.device.cert_path' can fall back to the root config key \
             'device.cert_path', but no root config was supplied"
        );
    }

    fn mapper_source() -> Box<dyn ConfigOps> {
        typed_source::<schemas::MapperConfigDto>(manager())
    }

    fn federated() -> FederatedConfig {
        FederatedConfig::new(Path::new("/nonexistent/tedge"))
    }
}

mod broken {
    use facet_config_runtime::*;

    facet_config_macro::define_config! {
        BrokenDevice {
            /// Falls back to a key that has no default and may not be set
            #[tedge_config(default(from_key = "name"))]
            id: String,

            /// A key with no default
            name: String,
        }
    }

    facet_config_macro::define_config! {
        BrokenHost {
            device: extern BrokenDeviceConfig,
        }
    }

    #[test]
    #[should_panic(expected = "invalid defaults registry")]
    fn dangling_required_fallback_in_a_mounted_schema_is_rejected_at_startup() {
        ConfigManager::from_schema::<BrokenHostConfig>(std::path::Path::new("/nonexistent/tedge"));
    }
}

fn typed_source<T>(manager: ConfigManager) -> Box<dyn ConfigOps>
where
    T: for<'a> facet::Facet<'a>
        + Default
        + serde::de::DeserializeOwned
        + serde::Serialize
        + 'static,
{
    Box::new(TypedConfigOps::<T>::new(manager, "/nonexistent/tedge/config.toml".into()).unwrap())
}
