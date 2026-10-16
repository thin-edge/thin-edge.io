use assert_matches::assert_matches;
use rstest::rstest;
use std::path::Path;

use tedge_config_engine::federated::FederatedConfig;
use tedge_config_engine::ops::Action;
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

#[rstest]
fn cloud_type_accepts_same_values_as_tedge_config_cloud_type(
    #[values("c8y", "az", "aws", "", "C8Y", "azure", "notacloud")] value: &str,
) {
    use crate::CloudType;
    use std::str::FromStr;
    use tedge_config::models::CloudType as TedgeConfigCloudType;

    let ours = CloudType::from_str(value).ok().map(|v| v.to_string());
    let theirs = TedgeConfigCloudType::from_str(value)
        .ok()
        .map(|v| v.to_string());
    assert_eq!(ours, theirs);
}

#[test]
fn cloud_type_can_be_set_and_read() {
    let dir = config_dir_with_mapper("thingsboard", "");
    let mut fed = crate::load_federated_config(dir.path()).unwrap();

    fed.mutate("mappers.thingsboard.cloud_type", Action::Set("c8y".into()))
        .unwrap();

    assert_eq!(
        fed.read("mappers.thingsboard.cloud_type")
            .unwrap()
            .as_deref(),
        Some("c8y")
    );
    let file: toml::Table = read_mapper_file(&dir, "thingsboard").parse().unwrap();
    assert_eq!(file["cloud_type"].as_str(), Some("c8y"));
}

#[test]
fn unknown_cloud_type_is_rejected() {
    let dir = config_dir_with_mapper("thingsboard", "");
    let mut fed = crate::load_federated_config(dir.path()).unwrap();

    let err = fed
        .mutate(
            "mappers.thingsboard.cloud_type",
            Action::Set("notacloud".into()),
        )
        .unwrap_err();

    let msg = err.to_string();
    assert!(
        msg.contains("notacloud"),
        "error should name the rejected value: {msg}"
    );
    assert_eq!(read_mapper_file(&dir, "thingsboard"), "");
}

#[test]
fn setting_cloud_type_leaves_other_keys_unchanged() {
    let dir = config_dir_with_mapper("thingsboard", "url = \"a.example.com:8883\"\n");
    let mut fed = crate::load_federated_config(dir.path()).unwrap();
    let keys_before: Vec<String> = fed.all_entries().into_iter().map(|e| e.key).collect();

    fed.mutate("mappers.thingsboard.cloud_type", Action::Set("c8y".into()))
        .unwrap();

    let file: toml::Table = read_mapper_file(&dir, "thingsboard").parse().unwrap();
    assert_eq!(file["url"].as_str(), Some("a.example.com:8883"));
    let fed = crate::load_federated_config(dir.path()).unwrap();
    let keys_after: Vec<String> = fed.all_entries().into_iter().map(|e| e.key).collect();
    assert_eq!(keys_after, keys_before);
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
        "TEDGE_MAPPERS_THINGSBOARD_URL".into(),
        "env.example.com:8883".into(),
    )]);

    let mut fed = FederatedConfig::new(dir.path());

    let root_mgr = ConfigManager::from_schema::<RootStubConfig>(dir.path());
    let mut root_ops =
        TypedConfigOps::<RootStubConfigDto>::new(root_mgr, dir.path().join("tedge.toml"));
    root_ops.apply_env_excluding(&env, &["mappers."]);
    fed.mount("", Box::new(root_ops)).unwrap();

    let mapper_mgr = ConfigManager::from_schema::<MapperConfig>(dir.path());
    let mut mapper_ops =
        TypedConfigOps::<MapperConfigDto>::new(mapper_mgr, mapper.join("mapper.toml"));
    mapper_ops.apply_env_with_prefix("TEDGE_MAPPERS_THINGSBOARD_", &env);
    fed.mount("mappers.thingsboard.", Box::new(mapper_ops))
        .unwrap();

    assert_eq!(
        fed.read("mappers.thingsboard.url").unwrap().as_deref(),
        Some("env.example.com:8883")
    );
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

#[test]
fn is_builtin_mapper_name_accepts_base_names() {
    assert!(crate::is_builtin_mapper_name("c8y"));
    assert!(crate::is_builtin_mapper_name("az"));
    assert!(crate::is_builtin_mapper_name("aws"));
}

#[test]
fn is_builtin_mapper_name_accepts_profiled_variants() {
    assert!(crate::is_builtin_mapper_name("c8y.prod"));
    assert!(crate::is_builtin_mapper_name("az.staging"));
    assert!(crate::is_builtin_mapper_name("aws.eu"));
}

#[test]
fn is_builtin_mapper_name_rejects_unknown_names() {
    assert!(!crate::is_builtin_mapper_name("thingsboard"));
    assert!(!crate::is_builtin_mapper_name(""));
    assert!(!crate::is_builtin_mapper_name("c8yy"));
    assert!(!crate::is_builtin_mapper_name("c8y."));
}

#[test]
fn extract_builtin_mapper_name_returns_base_name() {
    assert_eq!(
        crate::extract_builtin_mapper_name("mappers.c8y.url"),
        Some("c8y".to_owned()),
    );
}

#[test]
fn extract_builtin_mapper_name_returns_profile_directory_name() {
    assert_eq!(
        crate::extract_builtin_mapper_name("mappers.c8y.profiles.prod.url"),
        Some("c8y.prod".to_owned()),
    );
}

#[test]
fn extract_builtin_mapper_name_treats_a_key_without_profiles_as_a_base_mapper_key() {
    assert_eq!(
        crate::extract_builtin_mapper_name("mappers.c8y.prod.url"),
        Some("c8y".to_owned()),
    );
}

#[test]
fn extract_builtin_mapper_name_handles_nested_config_key() {
    assert_eq!(
        crate::extract_builtin_mapper_name("mappers.az.device.id"),
        Some("az".to_owned()),
    );
}

#[test]
fn extract_builtin_mapper_name_returns_none_for_non_mapper_key() {
    assert_eq!(crate::extract_builtin_mapper_name("device.id"), None);
}

#[test]
fn extract_builtin_mapper_name_returns_none_for_custom_mapper() {
    assert_eq!(
        crate::extract_builtin_mapper_name("mappers.thingsboard.url"),
        None,
    );
}

#[test]
fn profile_keys_are_read_and_written_under_profiles() {
    let dir = config_dir_with_mapper("c8y.prod", "");
    let mut fed = crate::load_federated_config(dir.path()).unwrap();

    fed.mutate(
        "mappers.c8y.profiles.prod.url",
        Action::Set("prod.example.com:8883".into()),
    )
    .unwrap();

    assert_eq!(
        fed.read("mappers.c8y.profiles.prod.url")
            .unwrap()
            .as_deref(),
        Some("prod.example.com:8883")
    );
    let file: toml::Table = read_mapper_file(&dir, "c8y.prod").parse().unwrap();
    assert_eq!(file["url"].as_str(), Some("prod.example.com:8883"));
}

#[test]
fn group_key_reaches_base_mapper_when_a_profile_is_named_after_the_group() {
    let dir = config_dir_with_mapper("c8y", "");
    add_mapper(&dir, "c8y.bridge", "url = \"bridge.example.com:8883\"\n");
    let mut fed = crate::load_federated_config(dir.path()).unwrap();

    fed.mutate(
        "mappers.c8y.bridge.clean_session",
        Action::Set("true".into()),
    )
    .unwrap();

    let file: toml::Table = read_mapper_file(&dir, "c8y").parse().unwrap();
    assert_eq!(file["bridge"]["clean_session"].as_bool(), Some(true));
    assert_eq!(
        read_mapper_file(&dir, "c8y.bridge"),
        "url = \"bridge.example.com:8883\"\n"
    );
}

#[test]
fn profile_key_without_profiles_is_an_unknown_key_of_the_base_mapper() {
    let dir = config_dir_with_mapper("c8y", "");
    add_mapper(&dir, "c8y.prod", "");
    let mut fed = crate::load_federated_config(dir.path()).unwrap();

    let err = fed
        .mutate(
            "mappers.c8y.prod.url",
            Action::Set("prod.example.com:8883".into()),
        )
        .unwrap_err();

    assert_matches!(err, ConfigError::UnknownKey(_));
}

#[rstest]
#[case::profile("c8y.prod.url", "mappers.c8y.profiles.prod.url")]
#[case::base_cloud("c8y.url", "mappers.c8y.url")]
#[case::custom_mapper("thingsboard.url", "mappers.thingsboard.url")]
fn mapper_command_key_becomes_its_mappers_key(#[case] key: &str, #[case] expected: &str) {
    let dir = config_dir_with_mapper("c8y.prod", "");
    add_mapper(&dir, "c8y", "");

    assert_eq!(crate::mapper_key_from_dir_key(key, dir.path()), expected);
}

#[test]
fn mapper_schema_has_no_profiles_group() {
    let keys = mapper_manager().keys::<MapperConfigDto>();
    assert!(
        keys.iter().all(|k| k.split('.').next() != Some("profiles")),
        "'profiles' is reserved for profile keys: {keys:?}"
    );
}

#[test]
fn profile_keys_are_overridden_by_env_variables_named_after_the_key() {
    let dir = config_dir_with_mapper("c8y.prod", "url = \"file.example.com:8883\"\n");
    let env = EnvOverrides::from_pairs([(
        "TEDGE_MAPPERS_C8Y_PROFILES_PROD_URL".into(),
        "env.example.com:8883".into(),
    )]);

    let fed = crate::schema::load_federated_config_with_env(dir.path(), &env).unwrap();

    assert_eq!(
        fed.read("mappers.c8y.profiles.prod.url")
            .unwrap()
            .as_deref(),
        Some("env.example.com:8883")
    );
}

#[test]
fn custom_mapper_keys_are_overridden_by_env_variables() {
    let dir = config_dir_with_mapper("my-mapper", "url = \"file.example.com:8883\"\n");
    let env = EnvOverrides::from_pairs([(
        "TEDGE_MAPPERS_MY_MAPPER_URL".into(),
        "env.example.com:8883".into(),
    )]);

    let fed = crate::schema::load_federated_config_with_env(dir.path(), &env).unwrap();

    assert_eq!(
        fed.read("mappers.my-mapper.url").unwrap().as_deref(),
        Some("env.example.com:8883")
    );
}

#[test]
fn mapper_directories_with_the_same_env_variable_name_fail_every_read_and_write() {
    let dir = config_dir_with_mapper("thingsboard.eu", "url = \"eu.example.com:8883\"\n");
    add_mapper(&dir, "thingsboard-eu", "url = \"eu.example.com:8883\"\n");
    add_mapper(&dir, "c8y", "url = \"c8y.example.com:8883\"\n");

    let mut fed =
        crate::schema::load_federated_config_with_env(dir.path(), &EnvOverrides::from_pairs([]))
            .unwrap();

    for key in ["mappers.thingsboard.eu.url", "mappers.thingsboard-eu.url"] {
        let msg = fed.read(key).unwrap_err().to_string();
        for expected in [
            "thingsboard.eu",
            "thingsboard-eu",
            "TEDGE_MAPPERS_THINGSBOARD_EU_",
        ] {
            assert!(
                msg.contains(expected),
                "reading {key} should name '{expected}': {msg}"
            );
        }
        fed.mutate(key, Action::Set("other.example.com:8883".into()))
            .unwrap_err();
    }
    assert_eq!(
        fed.read("mappers.c8y.url").unwrap().as_deref(),
        Some("c8y.example.com:8883")
    );
}

#[test]
fn set_is_refused_when_mapper_file_has_unknown_key() {
    let original = "url = \"a.example.com:8883\"\n\n[smartrest]\ntemplates = [\"x\"]\n";
    let dir = config_dir_with_mapper("c8y", original);
    let mut fed = crate::load_federated_config(dir.path()).unwrap();

    let err = fed
        .mutate("mappers.c8y.url", Action::Set("b.example.com:8883".into()))
        .unwrap_err();

    let msg = err.to_string();
    assert!(
        msg.contains("mappers/c8y/mapper.toml"),
        "error should name the file: {msg}"
    );
    assert!(
        msg.contains("smartrest.templates"),
        "error should name the unknown key: {msg}"
    );
    assert_eq!(read_mapper_file(&dir, "c8y"), original);
}

#[test]
fn get_still_works_when_mapper_file_has_unknown_key() {
    let dir = config_dir_with_mapper(
        "c8y",
        "url = \"a.example.com:8883\"\n\n[smartrest]\ntemplates = [\"x\"]\n",
    );
    let fed = crate::load_federated_config(dir.path()).unwrap();

    assert_eq!(
        fed.read("mappers.c8y.url").unwrap().as_deref(),
        Some("a.example.com:8883")
    );
}

#[test]
fn set_works_when_every_key_in_mapper_file_is_known() {
    let dir = config_dir_with_mapper(
        "thingsboard",
        "url = \"a.example.com:8883\"\n\n[bridge]\nclean_session = true\n",
    );
    let mut fed = crate::load_federated_config(dir.path()).unwrap();

    fed.mutate(
        "mappers.thingsboard.url",
        Action::Set("b.example.com:8883".into()),
    )
    .unwrap();

    let file: toml::Table = read_mapper_file(&dir, "thingsboard").parse().unwrap();
    assert_eq!(file["url"].as_str(), Some("b.example.com:8883"));
    assert_eq!(file["bridge"]["clean_session"].as_bool(), Some(true));
}

#[test]
fn set_on_another_mapper_succeeds_while_one_mapper_file_is_invalid() {
    let dir = config_dir_with_mapper("broken", "[bridge]\ntls.enable = \"on\"\n");
    add_mapper(&dir, "thingsboard", "");
    let mut fed = crate::load_federated_config(dir.path()).unwrap();

    fed.mutate(
        "mappers.thingsboard.url",
        Action::Set("a.example.com:8883".into()),
    )
    .unwrap();

    let file: toml::Table = read_mapper_file(&dir, "thingsboard").parse().unwrap();
    assert_eq!(file["url"].as_str(), Some("a.example.com:8883"));
}

#[test]
fn reading_the_invalid_mapper_names_its_file() {
    let dir = config_dir_with_mapper("broken", "[bridge]\ntls.enable = \"on\"\n");
    let fed = crate::load_federated_config(dir.path()).unwrap();

    let err = fed.read("mappers.broken.url").unwrap_err();

    let msg = err.to_string();
    assert!(
        msg.contains("mappers/broken/mapper.toml"),
        "error should name the file: {msg}"
    );
}

fn config_dir_with_mapper(name: &str, mapper_toml: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("tedge.toml"), "").unwrap();
    let mapper = dir.path().join("mappers").join(name);
    std::fs::create_dir_all(&mapper).unwrap();
    std::fs::write(mapper.join("mapper.toml"), mapper_toml).unwrap();
    dir
}

fn add_mapper(dir: &tempfile::TempDir, name: &str, mapper_toml: &str) {
    let mapper = dir.path().join("mappers").join(name);
    std::fs::create_dir_all(&mapper).unwrap();
    std::fs::write(mapper.join("mapper.toml"), mapper_toml).unwrap();
}

fn read_mapper_file(dir: &tempfile::TempDir, name: &str) -> String {
    std::fs::read_to_string(dir.path().join("mappers").join(name).join("mapper.toml")).unwrap()
}

fn mapper_manager() -> ConfigManager {
    ConfigManager::from_schema::<MapperConfig>(Path::new("/etc/tedge"))
}

fn root_manager() -> ConfigManager {
    ConfigManager::from_schema::<RootStubConfig>(Path::new("/etc/tedge"))
}

fn mapper_source(path: std::path::PathBuf) -> Box<dyn tedge_config_engine::ops::ConfigOps> {
    Box::new(TypedConfigOps::<MapperConfigDto>::new(
        mapper_manager(),
        path,
    ))
}

fn root_source(path: std::path::PathBuf) -> Box<dyn tedge_config_engine::ops::ConfigOps> {
    Box::new(TypedConfigOps::<RootStubConfigDto>::new(
        root_manager(),
        path,
    ))
}
