use tedge_config::tedge_toml::mapper_config::C8yMapperSpecificConfig;
use tedge_config::TEdgeConfig;
use tedge_config_macros::ProfileName;
use tedge_test_utils::fs::TempTedgeDir;

#[tokio::test]
async fn empty_new_config_uses_tedge_toml_defaults() {
    let ttd = TempTedgeDir::new();

    ttd.dir("mappers").file("c8y.toml").with_raw_content("");

    ttd.file("tedge.toml").with_toml_content(toml::toml! {
        [device]
        id = "device-from-tedge-toml"
        [c8y]
        url = "should-be-ignored.example.com"
    });

    let tedge_config = TEdgeConfig::load(ttd.path()).await.unwrap();
    let c8y_config = tedge_config
        .mapper_config::<C8yMapperSpecificConfig>(&None::<ProfileName>)
        .unwrap();

    assert!(
        c8y_config.http().or_none().is_none(),
        "HTTP URL should not be set"
    );
    assert!(
        c8y_config.mqtt().or_none().is_none(),
        "MQTT URL should not be set"
    );

    assert_eq!(
        c8y_config.device.id().unwrap(),
        "device-from-tedge-toml",
        "Device ID should come from tedge.toml defaults"
    );
}
