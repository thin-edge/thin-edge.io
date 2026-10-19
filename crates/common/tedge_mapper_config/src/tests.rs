use assert_matches::assert_matches;
use rstest::rstest;
use std::path::Path;
use tedge_test_utils::fs::TempTedgeDir;
use tedge_utils::paths::ManagedFile;
use tedge_utils::paths::TedgePaths;

use tedge_config_engine::federated::FederatedConfig;
use tedge_config_engine::ops::Action;
use tedge_config_engine::ops::ConfigOps;
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

#[tokio::test]
async fn cloud_type_can_be_set_and_read() {
    let dir = config_dir_with_mapper("thingsboard", "");
    let mut fed = load_without_env(&dir).await;

    fed.mutate("mappers.thingsboard.cloud_type", Action::Set("c8y".into()))
        .await
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

#[tokio::test]
async fn unknown_cloud_type_is_rejected() {
    let dir = config_dir_with_mapper("thingsboard", "");
    let mut fed = load_without_env(&dir).await;

    let err = fed
        .mutate(
            "mappers.thingsboard.cloud_type",
            Action::Set("notacloud".into()),
        )
        .await
        .unwrap_err();

    let msg = err.to_string();
    assert!(
        msg.contains("notacloud"),
        "error should name the rejected value: {msg}"
    );
    assert_eq!(read_mapper_file(&dir, "thingsboard"), "");
}

#[tokio::test]
async fn setting_cloud_type_leaves_other_keys_unchanged() {
    let dir = config_dir_with_mapper("thingsboard", "url = \"a.example.com:8883\"\n");
    let mut fed = load_without_env(&dir).await;
    let keys_before: Vec<String> = fed.all_entries().into_iter().map(|e| e.key).collect();

    fed.mutate("mappers.thingsboard.cloud_type", Action::Set("c8y".into()))
        .await
        .unwrap();

    let file: toml::Table = read_mapper_file(&dir, "thingsboard").parse().unwrap();
    assert_eq!(file["url"].as_str(), Some("a.example.com:8883"));
    let fed = load_without_env(&dir).await;
    let keys_after: Vec<String> = fed.all_entries().into_iter().map(|e| e.key).collect();
    assert_eq!(keys_after, keys_before);
}

#[tokio::test]
async fn from_root_references_resolve_against_root_stub() {
    let dir = config_dir_with_mapper("test", "");

    let mut fed = FederatedConfig::new(dir.std_path());
    fed.mount("", root_source(&dir).await).unwrap();
    fed.mount("mappers.test.", mapper_source(&dir, "test").await)
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

#[tokio::test]
async fn build_federated_config_discovers_mapper_directories() {
    let dir = config_dir_with_mapper("alpha", "url = \"alpha.example.com:8883\"\n");
    add_mapper(&dir, "beta", "url = \"beta.example.com:8883\"\n");

    let fed = load_without_env(&dir).await;

    assert_eq!(
        fed.read("mappers.alpha.url").unwrap().as_deref(),
        Some("alpha.example.com:8883")
    );
    assert_eq!(
        fed.read("mappers.beta.url").unwrap().as_deref(),
        Some("beta.example.com:8883")
    );
}

#[tokio::test]
async fn unknown_mapper_name_lists_known_mappers() {
    let dir = config_dir_with_mapper("thingsboard", "");

    let fed = load_without_env(&dir).await;

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

#[tokio::test]
async fn unknown_key_in_known_mapper_produces_error() {
    let dir = config_dir_with_mapper("test", "");

    let fed = load_without_env(&dir).await;

    let err = fed.read("mappers.test.nonexistent").unwrap_err();
    assert_matches!(err, ConfigError::UnknownKey(_));
}

#[tokio::test]
async fn env_variable_overrides_toml_value() {
    let dir = config_dir_with_mapper("thingsboard", "url = \"file.example.com:8883\"\n");

    let env = EnvOverrides::from_pairs([(
        "TEDGE_MAPPERS_THINGSBOARD_URL".into(),
        "env.example.com:8883".into(),
    )]);

    let mut fed = FederatedConfig::new(dir.std_path());

    let root_mgr = ConfigManager::from_schema::<RootStubConfig>(dir.std_path());
    let mut root_ops =
        TypedConfigOps::<RootStubConfigDto>::load(root_mgr, managed_file(&dir, "tedge.toml")).await;
    root_ops.apply_env_excluding(&env, &["mappers."]);
    fed.mount("", Box::new(root_ops)).unwrap();

    let mapper_mgr = ConfigManager::from_schema::<MapperConfig>(dir.std_path());
    let mut mapper_ops = TypedConfigOps::<MapperConfigDto>::load(
        mapper_mgr,
        managed_file(&dir, "mappers/thingsboard/mapper.toml"),
    )
    .await;
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

#[rstest]
#[case::cloud_key("mappers.c8y.url", Some("c8y"))]
#[case::profile_key("mappers.c8y.profiles.prod.url", Some("c8y.prod"))]
#[case::key_without_profiles("mappers.c8y.prod.url", Some("c8y"))]
#[case::nested_key("mappers.az.device.id", Some("az"))]
#[case::non_mapper_key("device.id", None)]
#[case::custom_mapper_key("mappers.thingsboard.url", None)]
fn finds_the_builtin_mapper_a_key_belongs_to(#[case] key: &str, #[case] expected: Option<&str>) {
    assert_eq!(crate::extract_builtin_mapper_name(key).as_deref(), expected);
}

#[tokio::test]
async fn profile_keys_are_read_and_written_under_profiles() {
    let dir = config_dir_with_mapper("c8y.prod", "");
    let mut fed = load_without_env(&dir).await;

    fed.mutate(
        "mappers.c8y.profiles.prod.url",
        Action::Set("prod.example.com:8883".into()),
    )
    .await
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

#[tokio::test]
async fn group_key_reaches_base_mapper_when_a_profile_is_named_after_the_group() {
    let dir = config_dir_with_mapper("c8y", "");
    add_mapper(&dir, "c8y.bridge", "url = \"bridge.example.com:8883\"\n");
    let mut fed = load_without_env(&dir).await;

    fed.mutate(
        "mappers.c8y.bridge.clean_session",
        Action::Set("true".into()),
    )
    .await
    .unwrap();

    let file: toml::Table = read_mapper_file(&dir, "c8y").parse().unwrap();
    assert_eq!(file["bridge"]["clean_session"].as_bool(), Some(true));
    assert_eq!(
        read_mapper_file(&dir, "c8y.bridge"),
        "url = \"bridge.example.com:8883\"\n"
    );
}

#[tokio::test]
async fn profile_key_without_profiles_is_an_unknown_key_of_the_base_mapper() {
    let dir = config_dir_with_mapper("c8y", "");
    add_mapper(&dir, "c8y.prod", "");
    let mut fed = load_without_env(&dir).await;

    let err = fed
        .mutate(
            "mappers.c8y.prod.url",
            Action::Set("prod.example.com:8883".into()),
        )
        .await
        .unwrap_err();

    assert_matches!(err, ConfigError::UnknownKey(_));
}

#[rstest]
#[case::profile("c8y.prod.url", "mappers.c8y.profiles.prod.url")]
#[case::base_cloud("c8y.url", "mappers.c8y.url")]
#[case::custom_mapper("thingsboard.url", "mappers.thingsboard.url")]
#[tokio::test]
async fn mapper_command_key_becomes_its_mappers_key(#[case] key: &str, #[case] expected: &str) {
    let dir = config_dir_with_mapper("c8y.prod", "");
    add_mapper(&dir, "c8y", "");

    assert_eq!(
        crate::mapper_key_from_dir_key(key, &config_root(&dir)).await,
        expected
    );
}

#[test]
fn mapper_schema_has_no_profiles_group() {
    let keys = mapper_manager().keys::<MapperConfigDto>();
    assert!(
        keys.iter().all(|k| k.split('.').next() != Some("profiles")),
        "'profiles' is reserved for profile keys: {keys:?}"
    );
}

#[tokio::test]
async fn profile_keys_are_overridden_by_env_variables_named_after_the_key() {
    let dir = config_dir_with_mapper("c8y.prod", "url = \"file.example.com:8883\"\n");
    let env = EnvOverrides::from_pairs([(
        "TEDGE_MAPPERS_C8Y_PROFILES_PROD_URL".into(),
        "env.example.com:8883".into(),
    )]);

    let fed = crate::schema::load_federated_config_with_env(&config_root(&dir), &env)
        .await
        .unwrap();

    assert_eq!(
        fed.read("mappers.c8y.profiles.prod.url")
            .unwrap()
            .as_deref(),
        Some("env.example.com:8883")
    );
}

#[tokio::test]
async fn custom_mapper_keys_are_overridden_by_env_variables() {
    let dir = config_dir_with_mapper("my-mapper", "url = \"file.example.com:8883\"\n");
    let env = EnvOverrides::from_pairs([(
        "TEDGE_MAPPERS_MY_MAPPER_URL".into(),
        "env.example.com:8883".into(),
    )]);

    let fed = crate::schema::load_federated_config_with_env(&config_root(&dir), &env)
        .await
        .unwrap();

    assert_eq!(
        fed.read("mappers.my-mapper.url").unwrap().as_deref(),
        Some("env.example.com:8883")
    );
}

#[tokio::test]
async fn mapper_directories_with_the_same_env_variable_name_fail_every_read_and_write() {
    let dir = config_dir_with_mapper("thingsboard.eu", "url = \"eu.example.com:8883\"\n");
    add_mapper(&dir, "thingsboard-eu", "url = \"eu.example.com:8883\"\n");
    add_mapper(&dir, "c8y", "url = \"c8y.example.com:8883\"\n");

    let mut fed = crate::schema::load_federated_config_with_env(
        &config_root(&dir),
        &EnvOverrides::from_pairs([]),
    )
    .await
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
            .await
            .unwrap_err();
    }
    assert_eq!(
        fed.read("mappers.c8y.url").unwrap().as_deref(),
        Some("c8y.example.com:8883")
    );
}

#[tokio::test]
async fn set_is_refused_when_mapper_file_has_unknown_key() {
    let original = "url = \"a.example.com:8883\"\n\n[smartrest]\ntemplates = [\"x\"]\n";
    let dir = config_dir_with_mapper("c8y", original);
    let mut fed = load_without_env(&dir).await;

    let err = fed
        .mutate("mappers.c8y.url", Action::Set("b.example.com:8883".into()))
        .await
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

#[tokio::test]
async fn get_still_works_when_mapper_file_has_unknown_key() {
    let dir = config_dir_with_mapper(
        "c8y",
        "url = \"a.example.com:8883\"\n\n[smartrest]\ntemplates = [\"x\"]\n",
    );
    let fed = load_without_env(&dir).await;

    assert_eq!(
        fed.read("mappers.c8y.url").unwrap().as_deref(),
        Some("a.example.com:8883")
    );
}

#[tokio::test]
async fn set_works_when_every_key_in_mapper_file_is_known() {
    let dir = config_dir_with_mapper(
        "thingsboard",
        "url = \"a.example.com:8883\"\n\n[bridge]\nclean_session = true\n",
    );
    let mut fed = load_without_env(&dir).await;

    fed.mutate(
        "mappers.thingsboard.url",
        Action::Set("b.example.com:8883".into()),
    )
    .await
    .unwrap();

    let file: toml::Table = read_mapper_file(&dir, "thingsboard").parse().unwrap();
    assert_eq!(file["url"].as_str(), Some("b.example.com:8883"));
    assert_eq!(file["bridge"]["clean_session"].as_bool(), Some(true));
}

#[rstest]
#[case::invalid_toml("[bridge\n")]
#[case::missing_value("url = \n")]
#[case::invalid_value("[bridge]\ntls.enable = \"on\"\n")]
#[tokio::test]
async fn set_on_another_mapper_succeeds_while_one_mapper_file_is_invalid(
    #[case] broken_file: &str,
) {
    let dir = config_dir_with_mapper("broken", broken_file);
    add_mapper(&dir, "thingsboard", "");
    let mut fed = load_without_env(&dir).await;

    fed.mutate(
        "mappers.thingsboard.url",
        Action::Set("a.example.com:8883".into()),
    )
    .await
    .unwrap();

    let file: toml::Table = read_mapper_file(&dir, "thingsboard").parse().unwrap();
    assert_eq!(file["url"].as_str(), Some("a.example.com:8883"));
}

#[rstest]
#[case::invalid_toml("[bridge\n")]
#[case::missing_value("url = \n")]
#[case::invalid_value("[bridge]\ntls.enable = \"on\"\n")]
#[tokio::test]
async fn reading_the_invalid_mapper_names_its_file(#[case] broken_file: &str) {
    let dir = config_dir_with_mapper("broken", broken_file);
    let fed = load_without_env(&dir).await;

    let err = fed.read("mappers.broken.url").unwrap_err();

    let msg = err.to_string();
    assert!(
        msg.contains("mappers/broken/mapper.toml"),
        "error should name the file: {msg}"
    );
}

#[rstest]
#[case::invalid_toml("[bridge\n")]
#[case::missing_value("url = \n")]
#[case::invalid_value("[bridge]\ntls.enable = \"on\"\n")]
#[tokio::test]
async fn setting_a_key_of_the_invalid_mapper_names_its_file(#[case] broken_file: &str) {
    let dir = config_dir_with_mapper("broken", broken_file);
    let mut fed = load_without_env(&dir).await;

    let err = fed
        .mutate(
            "mappers.broken.url",
            Action::Set("a.example.com:8883".into()),
        )
        .await
        .unwrap_err();

    assert_matches!(err, ConfigError::InvalidFile { .. });
    assert!(
        err.to_string().contains("mappers/broken/mapper.toml"),
        "error should name the file: {err}"
    );
    assert_eq!(read_mapper_file(&dir, "broken"), broken_file);
}

#[tokio::test]
async fn invalid_enum_value_is_rejected_naming_the_key_and_the_allowed_values() {
    let dir = config_dir_with_mapper("tb", "");
    let mut fed = load_without_env(&dir).await;

    let err = fed
        .mutate("mappers.tb.bridge.tls.enable", Action::Set("on".into()))
        .await
        .unwrap_err();

    let msg = err.to_string();
    assert!(
        msg.contains("mappers.tb.bridge.tls.enable"),
        "error should name the key: {msg}"
    );
    assert!(
        msg.contains("expected one of: true, false, auto"),
        "error should list the allowed values: {msg}"
    );
}

#[tokio::test]
async fn invalid_value_in_mapper_file_is_reported_with_its_key() {
    let dir = config_dir_with_mapper("broken", "[bridge]\ntls.enable = \"on\"\n");
    let fed = load_without_env(&dir).await;

    let err = fed.read("mappers.broken.url").unwrap_err();

    let msg = err.to_string();
    assert!(
        msg.contains("bridge.tls.enable"),
        "error should name the key: {msg}"
    );
    assert!(
        msg.contains("expected one of: true, false, auto"),
        "error should list the allowed values: {msg}"
    );
}

#[tokio::test]
async fn nested_keys_are_written_as_table_sections() {
    let dir = config_dir_with_mapper("tb", "");
    let mut fed = load_without_env(&dir).await;

    fed.mutate("mappers.tb.bridge.tls.enable", Action::Set("false".into()))
        .await
        .unwrap();

    assert_eq!(
        read_mapper_file(&dir, "tb"),
        "[bridge.tls]\nenable = \"false\"\n"
    );
}

#[rstest]
#[case::cloud_type("cloud_type", "c8y", "az")]
#[case::tls_enable("bridge.tls.enable", "false", "true")]
#[tokio::test]
async fn single_value_keys_are_replaced_by_add_and_cleared_only_by_removing_their_value(
    #[case] key: &str,
    #[case] value: &str,
    #[case] other_value: &str,
) {
    let dir = config_dir_with_mapper("tb", "");
    let mut fed = load_without_env(&dir).await;
    let key = format!("mappers.tb.{key}");

    fed.mutate(&key, Action::Add(other_value.into()))
        .await
        .unwrap();
    fed.mutate(&key, Action::Add(value.into())).await.unwrap();
    assert_eq!(fed.read(&key).unwrap().as_deref(), Some(value));

    fed.mutate(&key, Action::Remove(other_value.into()))
        .await
        .unwrap();
    assert_eq!(fed.read(&key).unwrap().as_deref(), Some(value));

    fed.mutate(&key, Action::Remove(value.into()))
        .await
        .unwrap();
    assert_ne!(fed.read(&key).unwrap().as_deref(), Some(value));
}

#[tokio::test]
async fn reading_a_mapper_whose_file_cannot_be_read_names_its_file() {
    let dir = TempTedgeDir::new();
    dir.file("tedge.toml");
    // A directory where the file should be fails to read with something other than "not found"
    dir.dir("mappers").dir("broken").dir("mapper.toml");
    let fed = load_without_env(&dir).await;

    let err = fed.read("mappers.broken.url").unwrap_err();

    assert!(
        err.to_string().contains("mappers/broken/mapper.toml"),
        "error should name the file: {err}"
    );
}

#[tokio::test]
async fn mapper_directories_with_the_same_env_variable_name_still_list_their_keys() {
    let dir = config_dir_with_mapper("thingsboard.eu", "");
    add_mapper(&dir, "thingsboard-eu", "");
    let fed = load_without_env(&dir).await;

    let keys: Vec<String> = fed.all_entries().into_iter().map(|e| e.key).collect();

    for key in ["mappers.thingsboard.eu.url", "mappers.thingsboard-eu.url"] {
        assert!(
            keys.iter().any(|k| k == key),
            "{key} should be listed: {keys:?}"
        );
    }
}

#[test]
fn builtin_mapper_without_a_directory_lists_its_keys_under_its_name() {
    let dir = TempTedgeDir::new();

    let keys: Vec<String> = crate::builtin_mapper_entries(dir.path(), "c8y")
        .into_iter()
        .map(|e| e.key)
        .collect();

    assert!(
        keys.iter().any(|k| k == "mappers.c8y.url"),
        "mappers.c8y.url should be listed: {keys:?}"
    );
    assert!(
        keys.iter().all(|k| k.starts_with("mappers.c8y.")),
        "every key should be under mappers.c8y.: {keys:?}"
    );
}

fn config_dir_with_mapper(name: &str, mapper_toml: &str) -> TempTedgeDir {
    let dir = TempTedgeDir::new();
    dir.file("tedge.toml");
    add_mapper(&dir, name, mapper_toml);
    dir
}

/// Loads the config ignoring the process environment, so exported `TEDGE_MAPPERS_*`
/// variables can't change the result
async fn load_without_env(dir: &TempTedgeDir) -> FederatedConfig {
    crate::schema::load_federated_config_with_env(&config_root(dir), &EnvOverrides::from_pairs([]))
        .await
        .unwrap()
}

fn add_mapper(dir: &TempTedgeDir, name: &str, mapper_toml: &str) {
    dir.dir("mappers")
        .dir(name)
        .file("mapper.toml")
        .with_raw_content(mapper_toml);
}

fn read_mapper_file(dir: &TempTedgeDir, name: &str) -> String {
    std::fs::read_to_string(mapper_file(dir, name)).unwrap()
}

fn mapper_file(dir: &TempTedgeDir, name: &str) -> camino::Utf8PathBuf {
    dir.path().join("mappers").join(name).join("mapper.toml")
}

fn mapper_manager() -> ConfigManager {
    ConfigManager::from_schema::<MapperConfig>(Path::new("/etc/tedge"))
}

fn root_manager() -> ConfigManager {
    ConfigManager::from_schema::<RootStubConfig>(Path::new("/etc/tedge"))
}

async fn mapper_source(dir: &TempTedgeDir, name: &str) -> Box<dyn ConfigOps> {
    let file = managed_file(dir, &format!("mappers/{name}/mapper.toml"));
    Box::new(TypedConfigOps::<MapperConfigDto>::load(mapper_manager(), file).await)
}

async fn root_source(dir: &TempTedgeDir) -> Box<dyn ConfigOps> {
    let file = managed_file(dir, "tedge.toml");
    Box::new(TypedConfigOps::<RootStubConfigDto>::load(root_manager(), file).await)
}

fn config_root(dir: &TempTedgeDir) -> TedgePaths {
    TedgePaths::from_root_with_defaults(dir.path(), "", "")
}

fn managed_file(dir: &TempTedgeDir, path: &str) -> ManagedFile {
    config_root(dir).file(path).unwrap()
}
