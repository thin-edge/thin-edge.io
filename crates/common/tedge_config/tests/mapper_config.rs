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

mod default_location_mode {
    use super::*;

    mod prefers_tedge_toml {
        use tedge_config::models::CloudType;

        use super::*;

        #[tokio::test]
        async fn mapper_config_is_not_created_if_tedge_toml_does_not_exist() {
            let ttd = TempTedgeDir::new();

            let tedge_config = TEdgeConfig::load(ttd.path()).await.unwrap();
            tedge_config
                .update_toml(&|dto, _rdr| {
                    dto.c8y.try_get_mut(None, "c8y").unwrap().url =
                        Some("example.com".parse().unwrap());
                    Ok(())
                })
                .await
                .unwrap();

            assert!(
                !ttd.path().join("mappers").exists(),
                "mappers dir should not have been created"
            );
        }

        #[tokio::test]
        async fn mapper_config_is_created_for_new_profile_of_existing_separate_config_cloud() {
            let ttd = TempTedgeDir::new();

            let tedge_config = TEdgeConfig::load(ttd.path()).await.unwrap();
            tedge_config
                .update_toml(&|dto, _rdr| {
                    dto.c8y.try_get_mut(None, "c8y").unwrap().url =
                        Some("example.com".parse().unwrap());
                    Ok(())
                })
                .await
                .unwrap();

            let tedge_config = TEdgeConfig::load(ttd.path()).await.unwrap();
            tedge_config
                .migrate_mapper_config(CloudType::C8y)
                .await
                .unwrap();

            let tedge_config = TEdgeConfig::load(ttd.path()).await.unwrap();
            tedge_config
                .update_toml(&|dto, _rdr| {
                    dto.c8y.try_get_mut(Some("new-profile"), "c8y").unwrap().url =
                        Some("new.example.com".parse().unwrap());
                    Ok(())
                })
                .await
                .unwrap();

            assert!(
                ttd.path().join("mappers").exists(),
                "mappers dir should exist"
            );
            let c8y_toml = tokio::fs::read_to_string(ttd.path().join("mappers/c8y.toml"))
                .await
                .unwrap();
            assert_eq!(c8y_toml.trim(), "url = \"example.com\"");
        }
    }

    mod prefers_separate_config {
        use super::*;

        #[tokio::test]
        async fn mapper_config_is_created_if_tedge_toml_does_not_exist() {
            let ttd = TempTedgeDir::new();

            let tedge_config = TEdgeConfig::load_prefer_separate_mapper_config(ttd.path())
                .await
                .unwrap();
            tedge_config
                .update_toml(&|dto, _rdr| {
                    dto.c8y.try_get_mut(None, "c8y").unwrap().url =
                        Some("example.com".parse().unwrap());
                    Ok(())
                })
                .await
                .unwrap();

            assert!(
                ttd.path().join("mappers").exists(),
                "mappers dir should have been created"
            );

            let tedge_toml = tokio::fs::read_to_string(ttd.path().join("tedge.toml"))
                .await
                .unwrap();
            let c8y_toml = tokio::fs::read_to_string(ttd.path().join("mappers/c8y.toml"))
                .await
                .unwrap();
            assert_eq!(tedge_toml, "");
            assert_eq!(c8y_toml.trim(), "url = \"example.com\"");
        }

        #[tokio::test]
        async fn mapper_config_is_created_if_tedge_toml_has_no_cloud_configs() {
            let ttd = TempTedgeDir::new();
            ttd.file("tedge.toml").with_toml_content(toml::toml!(
                device.type = "my-fancy-device"
            ));

            let tedge_config = TEdgeConfig::load_prefer_separate_mapper_config(ttd.path())
                .await
                .unwrap();
            tedge_config
                .update_toml(&|dto, _rdr| {
                    dto.c8y.try_get_mut(None, "c8y").unwrap().url =
                        Some("example.com".parse().unwrap());
                    Ok(())
                })
                .await
                .unwrap();

            assert!(
                ttd.path().join("mappers").exists(),
                "mappers dir should have been created"
            );

            let tedge_toml = tokio::fs::read_to_string(ttd.path().join("tedge.toml"))
                .await
                .unwrap();
            let c8y_toml = tokio::fs::read_to_string(ttd.path().join("mappers/c8y.toml"))
                .await
                .unwrap();
            assert!(!tedge_toml.contains("c8y"));
            assert_eq!(c8y_toml.trim(), "url = \"example.com\"");
        }

        #[tokio::test]
        async fn mapper_config_is_not_created_for_new_profile_of_existing_tedge_toml_cloud() {
            let ttd = TempTedgeDir::new();
            ttd.file("tedge.toml")
                .with_toml_content(toml::toml!(c8y.url = "example.com"));

            let tedge_config = TEdgeConfig::load_prefer_separate_mapper_config(ttd.path())
                .await
                .unwrap();
            tedge_config
                .update_toml(&|dto, _rdr| {
                    dto.c8y.try_get_mut(Some("new-profile"), "c8y").unwrap().url =
                        Some("new.example.com".parse().unwrap());
                    Ok(())
                })
                .await
                .unwrap();

            assert!(
                !ttd.path().join("mappers").exists(),
                "mappers dir should not have been created"
            );
        }

        #[tokio::test]
        async fn mapper_config_is_not_created_if_another_cloud_exists_in_tedge_toml() {
            let ttd = TempTedgeDir::new();
            ttd.file("tedge.toml")
                .with_toml_content(toml::toml!(az.url = "az.example.com"));

            let tedge_config = TEdgeConfig::load_prefer_separate_mapper_config(ttd.path())
                .await
                .unwrap();
            tedge_config
                .update_toml(&|dto, _rdr| {
                    dto.c8y.try_get_mut(None, "c8y").unwrap().url =
                        Some("c8y.example.com".parse().unwrap());
                    Ok(())
                })
                .await
                .unwrap();

            assert!(
                !ttd.path().join("mappers").exists(),
                "mappers dir should not have been created"
            );
        }
    }
}
