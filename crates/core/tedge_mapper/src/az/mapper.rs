use crate::core::component::TEdgeComponent;
use crate::core::mapper::start_basic_actors;
use crate::core::mqtt::configure_proxy;
use crate::flows_config;
use anyhow::Context;
use async_trait::async_trait;
use az_mapper_ext::AzureConverter;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::service_health_topic;
use tedge_config::tedge_toml::mapper_config::AzMapperSpecificConfig;
use tedge_config::tedge_toml::ProfileName;
use tedge_config::TEdgeConfig;
use tedge_file_system_ext::FsWatchActorBuilder;
use tedge_flows::FlowsMapperBuilder;
use tedge_mqtt_bridge::load_bridge_rules_from_directory;
use tedge_mqtt_bridge::persist_bridge_config_file;
use tedge_mqtt_bridge::rumqttc::Transport;
use tedge_mqtt_bridge::AuthMethod;
use tedge_mqtt_bridge::BridgeConfig;
use tedge_mqtt_bridge::MqttBridgeActorBuilder;
use tedge_utils::file::change_mode;
use tedge_utils::file::change_user_and_group;
use tedge_utils::file::create_directory_with_user_group;
use tedge_watch_ext::WatchActorBuilder;
use tracing::warn;
use yansi::Paint;

pub struct AzureMapper {
    pub profile: Option<ProfileName>,
}

#[async_trait]
impl TEdgeComponent for AzureMapper {
    async fn start(
        &self,
        tedge_config: TEdgeConfig,
        config_dir: &tedge_config::Path,
    ) -> Result<(), anyhow::Error> {
        let az_config = tedge_config.mapper_config::<AzMapperSpecificConfig>(&self.profile)?;
        let prefix = &az_config.bridge.topic_prefix;
        let az_mapper_name = format!("tedge-mapper-{prefix}");
        let (mut runtime, mut mqtt_actor) =
            start_basic_actors(&az_mapper_name, &tedge_config).await?;
        let mqtt_schema = MqttSchema::with_root(tedge_config.mqtt.topic_root.clone());

        if tedge_config.mqtt.bridge.built_in {
            let device_topic_id = tedge_config.mqtt.device_topic_id.clone();

            let remote_clientid = az_config.device.id()?;
            let rules = bridge_rules(&tedge_config, self.profile.as_ref()).await?;

            let mut cloud_config = tedge_mqtt_bridge::MqttOptions::new(
                &remote_clientid,
                az_config.url().or_config_not_set()?.to_string(),
                8883,
            );
            cloud_config.set_clean_session(false);
            cloud_config.set_credentials(
                format!(
                    "{}/{remote_clientid}/?api-version=2018-06-30",
                    az_config.url().or_config_not_set()?
                ),
                "",
            );
            cloud_config.set_keep_alive(az_config.bridge.keepalive_interval.duration());

            let tls_config = tedge_config
                .mqtt_client_config_rustls(&az_config)
                .context("Failed to create MQTT TLS config")?;
            cloud_config.set_transport(Transport::tls_with_config(tls_config.into()));

            configure_proxy(&tedge_config, &mut cloud_config)?;

            let built_in_bridge_name = format!("tedge-mapper-bridge-{prefix}");
            let health_topic =
                service_health_topic(&mqtt_schema, &device_topic_id, &built_in_bridge_name);

            let bridge_actor = MqttBridgeActorBuilder::new(
                &tedge_config,
                &built_in_bridge_name,
                &health_topic,
                rules,
                cloud_config,
                None,
            )
            .await;
            runtime.spawn(bridge_actor).await?;
        } else if tedge_config.proxy.address.or_none().is_some() {
            warn!("`proxy.address` is configured without the built-in bridge enabled. The bridge MQTT connection to the cloud will {} communicate via the configured proxy.", "not".bold())
        }
        let az_converter = AzureConverter::new(
            az_config.cloud_specific.mapper.timestamp,
            &mqtt_schema,
            az_config.cloud_specific.mapper.timestamp_format,
            prefix,
            az_config.mapper.mqtt.max_payload_size.0,
            az_config.topics.to_string(),
        );
        let mapper_dir = config_dir.join("mappers").join("az");
        let flows_dir =
            tedge_flows::flows_dir(config_dir, "az", self.profile.as_ref().map(|p| p.as_ref()));
        let mapper_config = crate::effective_mapper_config(&tedge_config, "az", mapper_dir).await?;
        let mut flows = crate::flow_registry(mapper_config, flows_dir).await?;
        az_converter.persist_builtin_flow(&mut flows).await?;
        let service_config = flows_config(&tedge_config, &az_mapper_name)?;
        let mut fs_actor = FsWatchActorBuilder::new();
        let mut cmd_watcher_actor = WatchActorBuilder::new();

        let mut flows_mapper = FlowsMapperBuilder::try_new(flows, service_config).await?;
        flows_mapper.connect(&mut mqtt_actor);
        flows_mapper.connect_fs(&mut fs_actor);
        flows_mapper.connect_cmd(&mut cmd_watcher_actor);

        runtime.spawn(flows_mapper).await?;
        runtime.spawn(fs_actor).await?;
        runtime.spawn(cmd_watcher_actor).await?;
        runtime.spawn(mqtt_actor).await?;
        runtime.run_to_completion().await?;
        Ok(())
    }
}

pub async fn resolve_effective_mapper_config(
    tedge_config: &TEdgeConfig,
    cloud_profile: Option<&ProfileName>,
) -> anyhow::Result<crate::custom::resolve::EffectiveMapperConfig> {
    let mapper_config_dir = tedge_config.mapper_config_dir::<AzMapperSpecificConfig>(cloud_profile);
    let az_reader = tedge_config.az_reader(cloud_profile.map(|p| p.as_ref()))?;
    let mapper_table = crate::custom::resolve::reader_to_toml_table(az_reader)?;
    let schema_json =
        serde_json::to_value(az_reader).context("failed to serialise az config to JSON")?;
    let mapper_config = crate::custom::config::load_mapper_config(&mapper_config_dir)
        .await?
        .unwrap_or_else(|| crate::custom::config::CustomMapperConfig {
            table: toml::Table::new(),
            cloud_type: None,
            url: None,
            device: None,
            bridge: crate::custom::config::BridgeConfig::default(),
            auth_method: crate::custom::config::AuthMethodConfig::Auto,
            credentials_path: None,
        });
    let mapper_name = match cloud_profile {
        Some(profile) => format!("az.{profile}"),
        None => "az".to_string(),
    };
    crate::custom::resolve::resolve_effective_config(
        &mapper_config,
        tedge_config,
        Some(&mapper_table),
        Some(schema_json),
    )
    .await
    .map(|c| c.with_mapper_name(mapper_name))
}

async fn bridge_rules(
    tedge_config: &TEdgeConfig,
    cloud_profile: Option<&ProfileName>,
) -> anyhow::Result<BridgeConfig> {
    let mapper_config_dir = tedge_config.mapper_config_dir::<AzMapperSpecificConfig>(cloud_profile);
    if let Err(err) =
        create_directory_with_user_group(mapper_config_dir.clone(), "tedge", "tedge", 0o755).await
    {
        warn!("failed to set file ownership for '{mapper_config_dir}': {err}");
    }

    let bridge_config_dir = mapper_config_dir.join("bridge");

    persist_bridge_config_file(
        &bridge_config_dir,
        "rules",
        include_str!("bridge/rules.toml"),
    )
    .await?;

    if let Err(err) = change_user_and_group(&bridge_config_dir, "tedge", "tedge").await {
        warn!("failed to set file ownership for '{bridge_config_dir}': {err}");
    }

    if let Err(err) = change_mode(&bridge_config_dir, 0o755).await {
        warn!("failed to set file permissions for '{bridge_config_dir}': {err}");
    }

    let effective = resolve_effective_mapper_config(tedge_config, cloud_profile).await?;

    load_bridge_rules_from_directory(
        &bridge_config_dir,
        tedge_config,
        AuthMethod::Certificate,
        cloud_profile,
        &effective,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tedge_test_utils::fs::TempTedgeDir;

    #[tokio::test]
    async fn bridge_rules_load_from_toml_with_correct_topics() {
        let ttd = create_test_dir("az.url = \"test.test.io\"").await;
        let (certificate, key) = make_self_signed_cert("test-device-id");
        let mapper_dir: camino::Utf8PathBuf = ttd.path().join("mappers/az").try_into().unwrap();
        tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
        tokio::fs::write(mapper_dir.join("cert.pem"), certificate.pem())
            .await
            .unwrap();
        tokio::fs::write(mapper_dir.join("key.pem"), key.serialize_pem())
            .await
            .unwrap();
        tokio::fs::write(
            mapper_dir.join("mapper.toml"),
            format!(
                "device.cert_path = \"{mapper_dir}/cert.pem\"\ndevice.key_path = \"{mapper_dir}/key.pem\"\n",
            ),
        )
        .await
        .unwrap();
        let config = TEdgeConfig::load(ttd.path()).await.unwrap();

        let rules = bridge_rules(&config, None).await.unwrap();

        // Device-to-cloud messages
        assert!(has_local_subscription(&rules, "az/messages/events/#"));

        // Cloud-to-device messages
        assert!(has_remote_subscription(
            &rules,
            "devices/test-device-id/messages/devicebound/#"
        ));

        // Direct methods
        assert!(has_local_subscription(&rules, "az/methods/res/#"));
        assert!(has_remote_subscription(&rules, "$iothub/methods/POST/#"));

        // Digital twin
        assert!(has_local_subscription(&rules, "az/twin/GET/#"));
        assert!(has_local_subscription(&rules, "az/twin/PATCH/#"));
        assert!(has_remote_subscription(&rules, "$iothub/twin/res/#"));
    }

    #[tokio::test]
    async fn custom_topic_prefix_applied() {
        let ttd =
            create_test_dir("az.url = \"test.test.io\"\naz.bridge.topic_prefix = \"custom-az\"")
                .await;
        let (certificate, key) = make_self_signed_cert("test-device-id");
        let mapper_dir: camino::Utf8PathBuf = ttd.path().join("mappers/az").try_into().unwrap();
        tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
        tokio::fs::write(mapper_dir.join("cert.pem"), certificate.pem())
            .await
            .unwrap();
        tokio::fs::write(mapper_dir.join("key.pem"), key.serialize_pem())
            .await
            .unwrap();
        tokio::fs::write(
            mapper_dir.join("mapper.toml"),
            format!(
                "device.cert_path = \"{mapper_dir}/cert.pem\"\ndevice.key_path = \"{mapper_dir}/key.pem\"\nbridge.topic_prefix = \"custom-az\"\n",
            ),
        )
        .await
        .unwrap();
        let config = TEdgeConfig::load(ttd.path()).await.unwrap();

        let rules = bridge_rules(&config, None).await.unwrap();

        assert!(has_local_subscription(
            &rules,
            "custom-az/messages/events/#"
        ));
        assert!(has_local_subscription(&rules, "custom-az/methods/res/#"));
    }

    async fn create_test_dir(toml: &str) -> TempTedgeDir {
        let ttd = TempTedgeDir::new();
        ttd.file("tedge.toml").with_raw_content(toml);
        ttd
    }

    fn make_self_signed_cert(cn: &str) -> (rcgen::Certificate, rcgen::KeyPair) {
        let key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).unwrap();
        let mut params = rcgen::CertificateParams::default();
        params.distinguished_name = rcgen::DistinguishedName::new();
        params
            .distinguished_name
            .push(rcgen::DnType::CommonName, cn);
        let issuer = rcgen::Issuer::from_params(&params, &key);
        let cert = params.signed_by(&key, &issuer).unwrap();
        (cert, key)
    }

    fn has_local_subscription(config: &BridgeConfig, topic: &str) -> bool {
        config.local_subscriptions().any(|t| t == topic)
    }

    fn has_remote_subscription(config: &BridgeConfig, topic: &str) -> bool {
        config.remote_subscriptions().any(|t| t == topic)
    }
}
