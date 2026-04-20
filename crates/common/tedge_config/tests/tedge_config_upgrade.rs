use tedge_config::TEdgeConfig;
use tedge_test_utils::fs::TempTedgeDir;

/// Test that backup is created before config upgrade with TOML value equality
#[tokio::test]
async fn backup_is_created_before_config_upgrade() {
    let ttd = TempTedgeDir::new();

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

    // Verify no backup exists initially
    assert!(
        tedge_config.check_backup_exists().is_none(),
        "Backup should not exist before upgrade"
    );

    // Get root dir before consuming tedge_config
    let root_dir = tedge_config.root_dir().to_path_buf();

    // Perform the upgrade (this creates the backup)
    let backup_path = tedge_config.migrate_mapper_configs().await.unwrap();

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

/// Test that subsequent upgrades overwrite previous backup
#[tokio::test]
async fn subsequent_upgrades_overwrite_previous_backup() {
    let ttd = TempTedgeDir::new();

    // First upgrade
    let first_config = toml::toml!(
        [config]
        version = "2"
        [device]
        type = "first-config"
    );
    ttd.file("tedge.toml")
        .with_toml_content(first_config.clone());

    let tedge_config = TEdgeConfig::load(ttd.path()).await.unwrap();
    let backup_path = tedge_config.migrate_mapper_configs().await.unwrap();

    let first_backup_config: toml::Table = toml::from_str(
        &tokio::fs::read_to_string(&backup_path)
            .await
            .expect("Should read first backup"),
    )
    .expect("Should parse first backup as TOML");

    assert_eq!(
        first_backup_config, first_config,
        "First backup should contain first configuration"
    );

    // Modify tedge.toml for second upgrade
    let second_config = toml::toml!(
        [config]
        version = "2"
        [device]
        type = "second-config"
    );
    ttd.file("tedge.toml")
        .with_toml_content(second_config.clone());

    // Second upgrade
    let tedge_config = TEdgeConfig::load(ttd.path()).await.unwrap();
    let backup_path = tedge_config.migrate_mapper_configs().await.unwrap();

    let second_backup_config: toml::Table = toml::from_str(
        &tokio::fs::read_to_string(&backup_path)
            .await
            .expect("Should read second backup"),
    )
    .expect("Should parse second backup as TOML");

    assert_eq!(
        second_backup_config, second_config,
        "Second backup should contain second configuration"
    );

    // Verify second backup doesn't contain first config
    assert_ne!(
        second_backup_config, first_config,
        "Second backup should not contain first configuration"
    );
}
