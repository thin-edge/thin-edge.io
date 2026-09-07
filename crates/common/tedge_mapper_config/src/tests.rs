use std::path::Path;

use tedge_config_engine::federated::FederatedConfig;
use tedge_config_engine::ops::TypedConfigOps;
use tedge_config_engine::ConfigError;
use tedge_config_engine::ConfigManager;
use tedge_config_engine::EnvOverrides;

use crate::schema::mapper::MapperConfig;
use crate::schema::mapper::MapperConfigDto;
use crate::schema::root_stub::RootStubConfig;
use crate::schema::root_stub::RootStubConfigDto;

#[test]
fn auth_method_accepts_same_values_as_auth_method_config() {
    use crate::AuthMethod;
    use std::str::FromStr;

    for value in ["auto", "certificate", "password", "basic"] {
        assert!(
            AuthMethod::from_str(value).is_ok(),
            "AuthMethod should accept '{value}'"
        );
    }

    assert_eq!(AuthMethod::from_str("auto").unwrap(), AuthMethod::Auto);
    assert_eq!(
        AuthMethod::from_str("certificate").unwrap(),
        AuthMethod::Certificate
    );
    assert_eq!(
        AuthMethod::from_str("password").unwrap(),
        AuthMethod::Password
    );
    assert_eq!(AuthMethod::from_str("basic").unwrap(), AuthMethod::Password);
}

#[test]
fn from_root_references_resolve_against_root_stub() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("tedge.toml"), "").unwrap();
    let mappers = dir.path().join("mappers/test");
    std::fs::create_dir_all(&mappers).unwrap();
    std::fs::write(mappers.join("mapper.toml"), "").unwrap();

    let mut fed = FederatedConfig::new(dir.path());
    fed.mount("", root_source(dir.path().join("tedge.toml")))
        .unwrap();
    fed.mount("mappers.test.", mapper_source(mappers.join("mapper.toml")))
        .unwrap();

    let cert = fed.read("mappers.test.device.cert_path").unwrap();
    assert_eq!(
        cert.as_deref(),
        Some("/etc/tedge/device-certs/tedge-certificate.pem")
    );

    let key = fed.read("mappers.test.device.key_path").unwrap();
    assert_eq!(
        key.as_deref(),
        Some("/etc/tedge/device-certs/tedge-private-key.pem")
    );

    let root = fed.read("mappers.test.device.root_cert_path").unwrap();
    assert_eq!(root.as_deref(), Some("/etc/ssl/certs"));
}

#[test]
fn build_federated_config_discovers_mapper_directories() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("tedge.toml"), "").unwrap();

    let alpha = dir.path().join("mappers/alpha");
    std::fs::create_dir_all(&alpha).unwrap();
    std::fs::write(
        alpha.join("mapper.toml"),
        "url = \"alpha.example.com:8883\"\n",
    )
    .unwrap();

    let beta = dir.path().join("mappers/beta");
    std::fs::create_dir_all(&beta).unwrap();
    std::fs::write(
        beta.join("mapper.toml"),
        "url = \"beta.example.com:8883\"\n",
    )
    .unwrap();

    let fed = crate::load_federated_config(dir.path()).unwrap();

    assert_eq!(
        fed.read("mappers.alpha.url").unwrap().as_deref(),
        Some("alpha.example.com:8883")
    );
    assert_eq!(
        fed.read("mappers.beta.url").unwrap().as_deref(),
        Some("beta.example.com:8883")
    );
}

#[test]
fn unknown_mapper_name_lists_known_mappers() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("tedge.toml"), "").unwrap();
    let known = dir.path().join("mappers/thingsboard");
    std::fs::create_dir_all(&known).unwrap();
    std::fs::write(known.join("mapper.toml"), "").unwrap();

    let fed = crate::load_federated_config(dir.path()).unwrap();

    let err = fed.read("mappers.noexist.url").unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("noexist"),
        "error should mention 'noexist': {msg}"
    );
    assert!(
        msg.contains("thingsboard"),
        "error should list known mapper 'thingsboard': {msg}"
    );
}

#[test]
fn unknown_key_in_known_mapper_produces_error() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("tedge.toml"), "").unwrap();
    let mapper = dir.path().join("mappers/test");
    std::fs::create_dir_all(&mapper).unwrap();
    std::fs::write(mapper.join("mapper.toml"), "").unwrap();

    let fed = crate::load_federated_config(dir.path()).unwrap();

    let err = fed.read("mappers.test.nonexistent").unwrap_err();
    assert!(matches!(err, ConfigError::UnknownKey(_)));
}

#[test]
fn env_variable_overrides_toml_value() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("tedge.toml"), "").unwrap();
    let mapper = dir.path().join("mappers/thingsboard");
    std::fs::create_dir_all(&mapper).unwrap();
    std::fs::write(
        mapper.join("mapper.toml"),
        "url = \"file.example.com:8883\"\n",
    )
    .unwrap();

    let env = EnvOverrides::from_pairs([(
        "TEDGE_CONFIG_MAPPERS_THINGSBOARD_URL".into(),
        "env.example.com:8883".into(),
    )]);

    let mut fed = FederatedConfig::new(dir.path());

    let root_mgr = ConfigManager::from_schema::<RootStubConfig>(dir.path());
    let mut root_ops =
        TypedConfigOps::<RootStubConfigDto>::new(root_mgr, dir.path().join("tedge.toml")).unwrap();
    root_ops.apply_env_excluding(&env, &["mappers."]);
    fed.mount("", Box::new(root_ops)).unwrap();

    let mapper_mgr = ConfigManager::from_schema::<MapperConfig>(dir.path());
    let mut mapper_ops =
        TypedConfigOps::<MapperConfigDto>::new(mapper_mgr, mapper.join("mapper.toml")).unwrap();
    mapper_ops.apply_env_with_prefix("TEDGE_CONFIG_MAPPERS_THINGSBOARD_", &env);
    fed.mount("mappers.thingsboard.", Box::new(mapper_ops))
        .unwrap();

    assert_eq!(
        fed.read("mappers.thingsboard.url").unwrap().as_deref(),
        Some("env.example.com:8883")
    );
}

#[test]
fn mapper_schema_keys_match_custom_mapper_config() {
    let mgr = mapper_manager();
    let mut keys: Vec<String> = mgr.keys::<MapperConfigDto>();
    keys.sort();

    let expected = vec![
        "auth_method",
        "bridge.clean_session",
        "bridge.keepalive_interval",
        "bridge.max_payload_size",
        "bridge.tls",
        "credentials_path",
        "device.cert_path",
        "device.id",
        "device.key_path",
        "device.root_cert_path",
        "url",
    ];

    assert_eq!(keys, expected);
}

#[test]
fn root_stub_defaults_match_real_tedge_defaults() {
    let mgr = root_manager();
    let dto = RootStubConfigDto::default();

    assert_eq!(
        mgr.read(&dto, "device.cert_path").unwrap().as_deref(),
        Some("/etc/tedge/device-certs/tedge-certificate.pem")
    );
    assert_eq!(
        mgr.read(&dto, "device.key_path").unwrap().as_deref(),
        Some("/etc/tedge/device-certs/tedge-private-key.pem")
    );
    assert_eq!(
        mgr.read(&dto, "device.root_cert_path").unwrap().as_deref(),
        Some("/etc/ssl/certs")
    );
}

fn mapper_manager() -> ConfigManager {
    ConfigManager::from_schema::<MapperConfig>(Path::new("/etc/tedge"))
}

fn root_manager() -> ConfigManager {
    ConfigManager::from_schema::<RootStubConfig>(Path::new("/etc/tedge"))
}

fn mapper_source(path: std::path::PathBuf) -> Box<dyn tedge_config_engine::ops::ConfigOps> {
    Box::new(TypedConfigOps::<MapperConfigDto>::new(mapper_manager(), path).unwrap())
}

fn root_source(path: std::path::PathBuf) -> Box<dyn tedge_config_engine::ops::ConfigOps> {
    Box::new(TypedConfigOps::<RootStubConfigDto>::new(root_manager(), path).unwrap())
}
