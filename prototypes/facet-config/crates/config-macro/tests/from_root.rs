//! Exercises `from_root` through the `define_config!` macro, the public
//! interface of the config library. Covers reading a `from_root` key with and
//! without a root config, building typed readers, and the validation that
//! runs when config sources are attached to a `FederatedConfig`: valid
//! references resolve, a reference to a nonexistent root key is rejected when
//! the source is mounted, a `from_root` user cannot be mounted before the
//! root config, and the root config itself cannot use `from_root`

use facet_config_runtime::federated::FederatedConfig;
use facet_config_runtime::ops::{ConfigOps, TypedConfigOps};
use facet_config_runtime::ConfigError;
use std::path::Path;

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

    pub fn manager() -> ConfigManager {
        ConfigManager::from_schema::<RootConfig>(std::path::Path::new("/nonexistent/tedge"))
    }

    pub fn source() -> Box<dyn ops::ConfigOps> {
        super::typed_source::<RootConfigDto>(manager())
    }
}

mod mapper {
    use facet_config_runtime::*;

    facet_config_macro::define_config! {
        Mapper {
            device: {
                /// Path to the device certificate for this mapper
                #[tedge_config(default(from_root = "device.cert_path"))]
                cert_path: String,
            },
        }
    }

    pub fn manager() -> ConfigManager {
        ConfigManager::from_schema::<MapperConfig>(std::path::Path::new("/nonexistent/tedge"))
    }

    pub fn source() -> Box<dyn ops::ConfigOps> {
        super::typed_source::<MapperConfigDto>(manager())
    }
}

mod misspelled_mapper {
    use facet_config_runtime::*;

    facet_config_macro::define_config! {
        MisspelledMapper {
            device: {
                /// Falls back to a key the root config does not define
                #[tedge_config(default(from_root = "device.certificate_path"))]
                cert_path: String,
            },
        }
    }

    pub fn source() -> Box<dyn ops::ConfigOps> {
        super::typed_source::<MisspelledMapperConfigDto>(manager())
    }

    fn manager() -> ConfigManager {
        ConfigManager::from_schema::<MisspelledMapperConfig>(std::path::Path::new(
            "/nonexistent/tedge",
        ))
    }
}

mod reading_from_root_keys {
    use super::*;

    #[test]
    fn read_resolves_through_the_supplied_root_config() {
        let mgr = mapper::manager();
        let dto = mapper::MapperConfigDto::default();
        assert_eq!(
            mgr.read_with_root(&dto, "device.cert_path", Some(&root_with_cert))
                .unwrap(),
            Some("/root/cert.pem".into())
        );
    }

    #[test]
    fn read_without_a_root_config_is_an_error() {
        let mgr = mapper::manager();
        let dto = mapper::MapperConfigDto::default();
        let err = mgr.read(&dto, "device.cert_path").unwrap_err();
        assert_eq!(
            err.to_string(),
            "'device.cert_path' can fall back to the root config key 'device.cert_path', \
             but no root config was supplied"
        );
    }

    #[test]
    fn explicitly_set_value_needs_no_root_config() {
        let mgr = mapper::manager();
        let mut dto = mapper::MapperConfigDto::default();
        mgr.set(&mut dto, "device.cert_path", "/mapper/cert.pem")
            .unwrap();
        assert_eq!(
            mgr.read(&dto, "device.cert_path").unwrap(),
            Some("/mapper/cert.pem".into())
        );
    }

    #[test]
    fn errors_from_the_root_config_propagate() {
        let mgr = mapper::manager();
        let dto = mapper::MapperConfigDto::default();
        let failing_root =
            |key: &str| Err::<Option<String>, _>(ConfigError::UnknownKey(key.to_owned()));
        let err = mgr
            .read_with_root(&dto, "device.cert_path", Some(&failing_root))
            .unwrap_err();
        assert!(
            matches!(&err, ConfigError::UnknownKey(key) if key == "device.cert_path"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn reader_field_resolves_through_the_supplied_root_config() {
        let mgr = mapper::manager();
        let dto = mapper::MapperConfigDto::default();
        let config: mapper::MapperConfig =
            mgr.build_reader(&dto, Some(&root_with_cert), "").unwrap();
        assert_eq!(
            config.device.cert_path.or_none(),
            Some(&"/root/cert.pem".to_string())
        );
    }

    #[test]
    fn building_a_reader_without_a_root_config_is_an_error() {
        let mgr = mapper::manager();
        let dto = mapper::MapperConfigDto::default();
        let err = mgr
            .build_reader::<_, mapper::MapperConfig>(&dto, None, "")
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "'device.cert_path' can fall back to the root config key 'device.cert_path', \
             but no root config was supplied"
        );
    }

    fn root_with_cert(key: &str) -> Result<Option<String>, ConfigError> {
        Ok(match key {
            "device.cert_path" => Some("/root/cert.pem".into()),
            _ => None,
        })
    }
}

mod mounting_into_a_federated_config {
    use super::*;

    #[test]
    fn valid_from_root_reference_resolves_through_the_root_config() {
        let mut fed = federated();
        fed.mount("", root::source()).unwrap();
        fed.mount("mappers.valid.", mapper::source()).unwrap();

        assert_eq!(
            fed.read("mappers.valid.device.cert_path").unwrap(),
            Some("/etc/tedge/cert.pem".into())
        );
    }

    #[test]
    fn from_root_reference_to_an_unknown_root_key_is_rejected_at_mount() {
        let mut fed = federated();
        fed.mount("", root::source()).unwrap();

        let err = fed
            .mount("mappers.bad.", misspelled_mapper::source())
            .unwrap_err();

        assert_eq!(
            err.to_string(),
            "'mappers.bad.device.cert_path' falls back to 'device.certificate_path', \
             which is not a key in the root config"
        );
    }

    #[test]
    fn from_root_user_cannot_be_mounted_before_the_root_config() {
        let mut fed = federated();

        let err = fed.mount("mappers.valid.", mapper::source()).unwrap_err();

        assert_eq!(
            err.to_string(),
            "'mappers.valid.device.cert_path' can fall back to the root config key \
             'device.cert_path', but no root config was supplied"
        );
    }

    #[test]
    fn root_config_cannot_use_from_root_defaults() {
        let mut fed = federated();

        let err = fed.mount("", mapper::source()).unwrap_err();

        assert_eq!(
            err.to_string(),
            "The root config cannot use from_root defaults, but 'device.cert_path' \
             falls back to 'device.cert_path'"
        );
    }

    fn federated() -> FederatedConfig {
        FederatedConfig::new(Path::new("/nonexistent/tedge"))
    }
}

fn typed_source<T>(manager: facet_config_runtime::ConfigManager) -> Box<dyn ConfigOps>
where
    T: for<'a> facet::Facet<'a>
        + Default
        + serde::de::DeserializeOwned
        + serde::Serialize
        + 'static,
{
    Box::new(TypedConfigOps::<T>::new(manager, "/nonexistent/tedge/config.toml".into()).unwrap())
}
