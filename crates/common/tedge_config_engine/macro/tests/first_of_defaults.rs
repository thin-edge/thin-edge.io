//! Exercises `default(...)` listing several sources: the first that gives a
//! value is used

use tedge_config_engine::ops::ConfigOps;
use tedge_config_engine::ops::TypedConfigOps;
use tedge_config_engine::ConfigError;
use tedge_config_engine::ConfigManager;
use tedge_utils::paths::TedgePaths;

mod schema {
    use tedge_config_engine::*;

    tedge_config_engine_macro::define_config! {
        FirstOf {
            device: {
                /// Path to the certificate
                cert_path: String,

                /// The certificate's name, or else the root config's device id
                #[tedge_config(default(
                    from_key_via(key = "device.cert_path", function = "common_name"),
                    from_root = "device.id",
                ))]
                id: String,
            },
        }
    }

    fn common_name(cert_path: &str) -> Result<Option<String>, String> {
        Ok((cert_path != "missing.pem").then(|| format!("cn-of-{cert_path}")))
    }
}

#[test]
fn the_first_source_that_gives_a_value_is_used() {
    let source = source_with_cert("cert.pem");

    assert_eq!(
        source.read("device.id", Some(&root)).unwrap().as_deref(),
        Some("cn-of-cert.pem")
    );
}

#[test]
fn a_later_source_is_used_when_the_earlier_ones_give_no_value() {
    let source = source_with_cert("missing.pem");

    assert_eq!(
        source.read("device.id", Some(&root)).unwrap().as_deref(),
        Some("root-device")
    );
}

#[test]
fn no_value_when_no_source_gives_one() {
    let source = source_with_cert("missing.pem");

    assert_eq!(
        source.read("device.id", Some(&no_root_value)).unwrap(),
        None
    );
}

#[test]
fn an_explicit_value_beats_every_source() {
    let mut source = source_with_cert("cert.pem");
    manager()
        .set(source.dto_mut(), "device.id", "explicit")
        .unwrap();

    assert_eq!(
        source.read("device.id", Some(&root)).unwrap().as_deref(),
        Some("explicit")
    );
}

#[test]
fn every_root_key_a_source_falls_back_to_is_a_root_dependency() {
    let root_keys: Vec<_> = manager()
        .root_dependencies()
        .into_iter()
        .map(|d| d.root_key)
        .collect();

    assert_eq!(root_keys, ["device.id"]);
}

fn root(key: &str) -> Result<Option<String>, ConfigError> {
    Ok((key == "device.id").then(|| "root-device".to_owned()))
}

fn no_root_value(_key: &str) -> Result<Option<String>, ConfigError> {
    Ok(None)
}

fn source_with_cert(cert_path: &str) -> TypedConfigOps<schema::FirstOfConfigDto> {
    let file = TedgePaths::from_root_with_defaults("/nonexistent/tedge", "", "")
        .file("config.toml")
        .unwrap();
    let mut source = TypedConfigOps::empty(manager(), file);
    manager()
        .set(source.dto_mut(), "device.cert_path", cert_path)
        .unwrap();
    source
}

fn manager() -> ConfigManager {
    ConfigManager::from_schema::<schema::FirstOfConfig>(std::path::Path::new("/etc/tedge"))
}
