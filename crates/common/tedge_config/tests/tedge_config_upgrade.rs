use tedge_config::TEdgeConfig;
use tedge_test_utils::fs::TempTedgeDir;

/// Test that backup is created before config upgrade with cloud config present
#[tokio::test]
async fn backup_is_created_before_config_upgrade() {
    let ttd = config_root();

    // Set up multi-cloud configuration before upgrade
    let original_config = toml::toml!(
        [config]
        version = "2"
        [device]
        id = "multi-cloud-device"
        [c8y]
        url = "c8y.example.com"
        [az]
        url = "az.example.com"
        [aws]
        url = "aws.example.com"
    );
    ttd.file("tedge.toml")
        .with_toml_content(original_config.clone());

    let tedge_config = TEdgeConfig::load(ttd.path()).await.unwrap();

    let backup_path = ttd.path().join("tedge.toml.bak");
    // Verify no backup exists initially
    assert!(
        !backup_path.exists(),
        "Backup should not exist before upgrade"
    );

    // Get root dir before consuming tedge_config
    let root_dir = tedge_config.root_dir().to_path_buf();

    // Perform the upgrade (this creates the backup)
    let backup_path = tedge_config
        .migrate_mapper_configs()
        .await
        .expect("Upgrade should succeed")
        .expect("Backup file is created");

    // Verify backup was created
    assert!(
        backup_path.exists(),
        "Backup file should exist after upgrade at {}",
        backup_path
    );

    // Verify backup has correct filename
    assert_eq!(
        backup_path.file_name(),
        Some("tedge.toml.bak"),
        "Backup should have .bak extension"
    );

    // Verify backup is in the correct directory
    assert_eq!(
        backup_path.parent().unwrap(),
        root_dir,
        "Backup should be in config root directory"
    );

    // Verify backup content has the same TOML values as original (not necessarily the same formatting)
    let backup_content = tokio::fs::read_to_string(&backup_path)
        .await
        .expect("Should read backup file");
    let backup_config: toml::Table =
        toml::from_str(&backup_content).expect("Should parse backup content as TOML");

    assert_eq!(
        original_config, backup_config,
        "Backup TOML values should equal original configuration values"
    );
}

/// Test that second upgrade with nothing to migrate returns None and preserves backup
#[tokio::test]
async fn second_upgrade_with_nothing_to_migrate_preserves_first_backup() {
    let ttd = config_root();

    // First upgrade with cloud config
    let first_config = toml::toml!(
        [config]
        version = "2"
        [device]
        id = "test-device"
        [c8y]
        url = "c8y.example.com"
    );
    ttd.file("tedge.toml")
        .with_toml_content(first_config.clone());

    let tedge_config = TEdgeConfig::load(ttd.path()).await.unwrap();
    let backup_path = tedge_config
        .migrate_mapper_configs()
        .await
        .expect("First upgrade should succeed")
        .expect("First migration should create backup");

    // Read the backup content after first migration
    let first_backup_content = tokio::fs::read_to_string(&backup_path)
        .await
        .expect("Should read backup file after first migration");

    // Second upgrade with nothing to migrate (cloud config is now gone from tedge.toml)
    let tedge_config = TEdgeConfig::load(ttd.path()).await.unwrap();
    let second_result = tedge_config.migrate_mapper_configs().await.unwrap();

    // Second migration should return None since nothing was migrated
    assert!(
        second_result.is_none(),
        "Second migration should return None when nothing needs to migrate"
    );

    // Verify backup file still exists and content is unchanged
    let second_backup_content = tokio::fs::read_to_string(&backup_path)
        .await
        .expect("Backup should still exist after second migration");

    assert_eq!(
        first_backup_content, second_backup_content,
        "Backup content should be unchanged after second migration with nothing to migrate"
    );
}

/// Test that sequential migrations overwrite backup with pre-migration state
#[tokio::test]
async fn second_upgrade_with_migration_overwrites_previous_backup() {
    let ttd = config_root();

    // First upgrade
    let first_config = toml::toml!(
        [config]
        version = "2"
        [device]
        id = "test-device"
        [c8y]
        url = "c8y.example.com"
    );
    ttd.file("tedge.toml")
        .with_toml_content(first_config.clone());

    let tedge_config = TEdgeConfig::load(ttd.path()).await.unwrap();
    let backup_path = tedge_config
        .migrate_mapper_configs()
        .await
        .expect("First migration should succeed")
        .expect("First migration should create backup");

    // Verify first backup contains the original c8y config
    let first_backup_content = tokio::fs::read_to_string(&backup_path)
        .await
        .expect("Should read backup file after first migration");
    let first_backup_config: toml::Table =
        toml::from_str(&first_backup_content).expect("Should parse first backup as TOML");
    assert_eq!(
        first_backup_config, first_config,
        "First backup should contain original config with cloud section"
    );

    // Update the tedge.toml to simulate a new config version that requires migration
    let second_config = toml::toml!(
        [config]
        version = "2"
        [device]
        id = "test-device"
        [az]
        url = "az.example.com"
    );
    ttd.file("tedge.toml")
        .with_toml_content(second_config.clone());

    // Second upgrade
    let tedge_config = TEdgeConfig::load(ttd.path()).await.unwrap();
    let backup_path = tedge_config
        .migrate_mapper_configs()
        .await
        .expect("Second migration should succeed")
        .expect("Second migration should create backup");

    // Verify second backup contains the state before the second migration (with az config)
    let second_backup_content = tokio::fs::read_to_string(&backup_path)
        .await
        .expect("Should read backup file after second migration");
    let second_backup_config: toml::Table =
        toml::from_str(&second_backup_content).expect("Should parse second backup as TOML");
    assert_eq!(
        second_backup_config, second_config,
        "Second backup should contain the state before second migration (with az config)"
    );

    // Verify second backup doesn't contain first config
    assert_ne!(
        second_backup_config, first_config,
        "Second backup should not contain first configuration"
    );
}

fn config_root() -> TempTedgeDir {
    let ttd = TempTedgeDir::new();

    // Create system.toml with empty user and group to use current process ownership
    ttd.file("system.toml").with_toml_content(toml::toml! {
        user = ""
        group = ""
    });

    ttd
}
