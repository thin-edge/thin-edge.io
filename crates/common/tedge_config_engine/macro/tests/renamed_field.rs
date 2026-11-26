//! Exercises a field renamed within its group: files written under the old
//! name still read, and are rewritten under the new one

use tedge_config_engine::ops::Action;
use tedge_config_engine::ops::ConfigOps;
use tedge_config_engine::ops::TypedConfigOps;
use tedge_config_engine::ConfigError;
use tedge_config_engine::ConfigManager;
use tedge_test_utils::fs::TempTedgeDir;
use tedge_utils::paths::ManagedFile;
use tedge_utils::paths::TedgePaths;

mod schema {
    use tedge_config_engine::*;

    tedge_config_engine_macro::define_config! {
        Renamed {
            enable: {
                /// Enable log upload
                #[tedge_config(default(value = "true"), deprecated_key = "enable.log_management")]
                log_upload: bool,

                /// Enable config snapshot
                #[tedge_config(default(value = "true"))]
                config_snapshot: bool,
            },

            /// Moved here from another group
            #[tedge_config(deprecated_key = "old.port")]
            port: u16,
        }
    }
}

#[tokio::test]
async fn a_file_using_the_old_name_reads_under_the_new_name() {
    let (_dir, file) = file_with("[enable]\nlog_management = false\n");

    let source = load(file).await;

    assert_eq!(
        source.read("enable.log_upload", None).unwrap().as_deref(),
        Some("false")
    );
}

#[tokio::test]
async fn writing_a_file_using_the_old_name_rewrites_it_under_the_new_name() {
    let (_dir, file) = file_with("[enable]\nlog_management = false\n");
    let mut source = load(file.clone()).await;

    source
        .mutate("enable.config_snapshot", Action::Set("false".into()))
        .await
        .unwrap();

    let written: toml::Table = std::fs::read_to_string(file.path())
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(
        written["enable"].as_table().unwrap(),
        &toml::toml! {
            log_upload = false
            config_snapshot = false
        }
    );
}

#[tokio::test]
async fn a_key_moved_from_another_group_is_unknown_in_files() {
    let (_dir, file) = file_with("[old]\nport = 1883\n");
    let mut source = load(file).await;

    let err = source
        .mutate("enable.config_snapshot", Action::Set("false".into()))
        .await
        .unwrap_err();

    assert!(
        matches!(&err, ConfigError::UnknownKeysInFile { keys, .. } if keys == &["old.port"]),
        "unexpected error: {err:?}"
    );
}

async fn load(file: ManagedFile) -> TypedConfigOps<schema::RenamedConfigDto> {
    let manager =
        ConfigManager::from_schema::<schema::RenamedConfig>(std::path::Path::new("/etc/tedge"));
    TypedConfigOps::load(manager, file).await
}

fn file_with(content: &str) -> (TempTedgeDir, ManagedFile) {
    let dir = TempTedgeDir::new();
    dir.file("config.toml").with_raw_content(content);
    let file = TedgePaths::from_root_with_defaults(dir.path(), "", "")
        .file("config.toml")
        .unwrap();
    (dir, file)
}
