//! Exercises what a write leaves in the file

use tedge_config_engine::ops::Action;
use tedge_config_engine::ops::ConfigOps;
use tedge_config_engine::ops::TypedConfigOps;
use tedge_config_engine::ConfigManager;
use tedge_test_utils::fs::TempTedgeDir;
use tedge_utils::paths::ManagedFile;
use tedge_utils::paths::TedgePaths;

mod schema {
    use tedge_config_engine::*;

    tedge_config_engine_macro::define_config! {
        Saved {
            /// A top-level key
            url: String,

            bridge: {
                include: {
                    /// A key two groups deep
                    local_cleansession: bool,
                },
            },
        }
    }
}

#[tokio::test]
async fn unsetting_the_last_key_of_a_group_removes_the_group() {
    let (_dir, file) = file_with("url = \"example.com\"\n");
    let mut source = load(file.clone()).await;

    source
        .mutate(
            "bridge.include.local_cleansession",
            Action::Set("true".into()),
        )
        .await
        .unwrap();
    source
        .mutate("bridge.include.local_cleansession", Action::Unset)
        .await
        .unwrap();

    assert_eq!(
        std::fs::read_to_string(file.path()).unwrap(),
        "url = \"example.com\"\n"
    );
}

async fn load(file: ManagedFile) -> TypedConfigOps<schema::SavedConfigDto> {
    let manager =
        ConfigManager::from_schema::<schema::SavedConfig>(std::path::Path::new("/etc/tedge"));
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
