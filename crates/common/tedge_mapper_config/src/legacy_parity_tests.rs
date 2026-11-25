//! Differential tests pinning the facet schemas to the `tedge_config` tables
//! they transliterate, until `tedge_config` is deleted

use rstest::rstest;
use std::collections::BTreeMap;
use std::str::FromStr;
use tedge_config::tedge_toml::ReadableKey;
use tedge_config::tedge_toml::READABLE_KEYS;
use tedge_config::TEdgeConfig;
use tedge_config_engine::federated::FederatedConfig;
use tedge_config_engine::ConfigManager;
use tedge_config_engine::EnvOverrides;
use tedge_test_utils::fs::TempTedgeDir;
use tedge_utils::paths::TedgePaths;

use crate::root::RootConfig;
use crate::root::RootConfigDto;

#[test]
fn root_device_table_has_the_same_keys_as_tedge_config() {
    let legacy: Vec<_> = legacy_docs("device.").into_keys().collect();
    let ours: Vec<_> = root_docs("device.").into_keys().collect();

    assert_eq!(ours, legacy);
}

#[test]
fn root_device_table_has_the_same_docs_and_examples_as_tedge_config() {
    assert_eq!(root_docs("device."), legacy_docs("device."));
}

#[rstest]
#[case::with_certificate(true)]
#[case::without_certificate(false)]
#[tokio::test]
async fn root_device_table_has_the_same_defaults_as_tedge_config(#[case] with_cert: bool) {
    let dir = TempTedgeDir::new();
    dir.file("tedge.toml");
    if with_cert {
        write_certificate(&dir, "device-certs/tedge-certificate.pem", "test-device");
    }

    let keys: Vec<_> = root_docs("device.").into_keys().collect();
    assert_eq!(read_ours(&dir, &keys).await, read_legacy(&dir, &keys).await);
}

#[rstest]
#[case::device_id("device.id", "\"my-device\"")]
#[case::device_type("device.type", "\"my-type\"")]
#[case::key_path("device.key_path", "\"/tmp/key.pem\"")]
#[case::cryptoki_mode("device.cryptoki.mode", "\"socket\"")]
#[case::batch_size("device.cryptoki.find_objects_batch_size", "10")]
#[tokio::test]
async fn root_device_values_set_in_tedge_toml_read_the_same_as_tedge_config(
    #[case] key: &str,
    #[case] toml_value: &str,
) {
    let dir = TempTedgeDir::new();
    let (group, name) = key.rsplit_once('.').unwrap();
    dir.file("tedge.toml")
        .with_raw_content(&format!("[{group}]\n{name} = {toml_value}\n"));

    let keys = [key.to_owned()];
    assert_eq!(read_ours(&dir, &keys).await, read_legacy(&dir, &keys).await);
}

#[tokio::test]
async fn explicit_device_id_beats_the_certificate_common_name_unlike_tedge_config() {
    let dir = TempTedgeDir::new();
    dir.file("tedge.toml")
        .with_raw_content("[device]\nid = \"explicit\"\n");
    write_certificate(&dir, "device-certs/tedge-certificate.pem", "from-cert");

    let keys = ["device.id".to_owned()];
    assert_eq!(
        read_ours(&dir, &keys).await["device.id"].as_deref(),
        Some("explicit")
    );
    assert_eq!(
        read_legacy(&dir, &keys).await["device.id"].as_deref(),
        Some("from-cert")
    );
}

#[rstest]
fn absolute_path_accepts_same_values_as_tedge_config_absolute_path(
    #[values("/etc/tedge", "/etc/../tmp/./x", "relative/path", "", "/")] value: &str,
) {
    let ours = crate::AbsolutePath::from_str(value)
        .ok()
        .map(|v| v.to_string());
    let theirs = tedge_config::models::AbsolutePath::from_str(value)
        .ok()
        .map(|v| v.to_string());
    assert_eq!(ours, theirs);
}

#[rstest]
fn cryptoki_accepts_same_values_as_tedge_config_cryptoki(
    #[values("off", "socket", "module", "", "OFF", "on")] value: &str,
) {
    let ours = crate::Cryptoki::from_str(value).ok().map(|v| v.to_string());
    let theirs = tedge_config::models::Cryptoki::from_str(value)
        .ok()
        .map(|v| v.to_string());
    assert_eq!(ours, theirs);
}

/// Docs and examples of every `tedge_config` key under `prefix`, with the
/// whitespace of the doc comment normalised
fn legacy_docs(prefix: &str) -> BTreeMap<String, (String, Vec<String>)> {
    READABLE_KEYS
        .iter()
        .filter(|(key, _)| key.starts_with(prefix))
        .map(|(key, ty)| {
            let mut doc = ty.comment.map(normalise).unwrap_or_default();
            if let Some(note) = ty.metas.get("note") {
                doc = format!("{doc} {}", normalise(note));
            }
            let examples = match ty.example {
                Some(doku::Example::Simple(v) | doku::Example::Literal(v)) => vec![v.to_owned()],
                Some(doku::Example::Compound(vs)) => vs.iter().map(|v| v.to_string()).collect(),
                None => Vec::new(),
            };
            (key.to_string(), (doc, examples))
        })
        .collect()
}

fn root_docs(prefix: &str) -> BTreeMap<String, (String, Vec<String>)> {
    ConfigManager::from_schema::<RootConfig>(std::path::Path::new("/etc/tedge"))
        .key_entries::<RootConfigDto>()
        .into_iter()
        .filter(|e| e.key.starts_with(prefix))
        .map(|e| {
            let examples = e.examples.iter().map(|v| v.to_string()).collect();
            (e.key, (normalise(&e.doc.join("\n")), examples))
        })
        .collect()
}

fn normalise(doc: &str) -> String {
    doc.split_whitespace().collect::<Vec<_>>().join(" ")
}

async fn read_ours(dir: &TempTedgeDir, keys: &[String]) -> BTreeMap<String, Option<String>> {
    let fed = federated_config(dir).await;
    keys.iter()
        .map(|key| (key.clone(), fed.read(key).ok().flatten()))
        .collect()
}

async fn read_legacy(dir: &TempTedgeDir, keys: &[String]) -> BTreeMap<String, Option<String>> {
    let config = TEdgeConfig::load(dir.path()).await.unwrap();
    keys.iter()
        .map(|key| {
            let readable = ReadableKey::from_str(key).unwrap();
            (key.clone(), config.read_string(&readable).ok())
        })
        .collect()
}

async fn federated_config(dir: &TempTedgeDir) -> FederatedConfig {
    let root = TedgePaths::from_root_with_defaults(dir.path(), "", "");
    crate::schema::load_federated_config_with_env(&root, &EnvOverrides::from_pairs([]))
        .await
        .unwrap()
}

fn write_certificate(dir: &TempTedgeDir, path: &str, common_name: &str) {
    use certificate::CsrTemplate;
    use certificate::KeyCertPair;
    use certificate::KeyKind;

    let cert = KeyCertPair::new_selfsigned_certificate(
        &CsrTemplate::default(),
        common_name,
        &KeyKind::New,
    )
    .unwrap();
    let path = dir.path().join(path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, cert.certificate_pem_string().unwrap()).unwrap();
}
