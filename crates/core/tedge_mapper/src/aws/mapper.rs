use crate::core::component::TEdgeComponent;
use crate::core::mapper::start_basic_actors;
use crate::core::mqtt::configure_proxy;
use crate::flows_config;
use anyhow::Context;
use async_trait::async_trait;
use aws_mapper_ext::AwsConverter;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::service_health_topic;
use tedge_config::tedge_toml::mapper_config::AwsMapperSpecificConfig;
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
use tedge_watch_ext::WatchActorBuilder;
use tracing::warn;
use yansi::Paint;

pub struct AwsMapper {
    pub profile: Option<ProfileName>,
}

impl AwsMapper {
    /// Returns the mapper directory path for this instance.
    pub fn mapper_dir(&self, config_dir: &Utf8Path) -> Utf8PathBuf {
        crate::mapper_dir(config_dir, "aws", self.profile.as_ref())
    }
}

#[async_trait]
impl TEdgeComponent for AwsMapper {
    async fn start(
        &self,
        tedge_config: TEdgeConfig,
        config_dir: &tedge_config::Path,
    ) -> Result<(), anyhow::Error> {
        let aws_config = tedge_config.mapper_config::<AwsMapperSpecificConfig>(&self.profile)?;
        let prefix = &aws_config.bridge.topic_prefix;
        let aws_mapper_name = format!("tedge-mapper-{prefix}");
        let (mut runtime, mut mqtt_actor) =
            start_basic_actors(&aws_mapper_name, &tedge_config).await?;
        let mqtt_schema = MqttSchema::with_root(tedge_config.mqtt.topic_root.clone());

        if tedge_config.mqtt.bridge.built_in {
            let device_id = aws_config.device.id()?;
            let device_topic_id = tedge_config.mqtt.device_topic_id.clone();

            let rules = bridge_rules(&tedge_config, self.profile.as_ref()).await?;

            let mut cloud_config = tedge_mqtt_bridge::MqttOptions::new(
                device_id,
                aws_config.url().or_config_not_set()?.to_string(),
                8883,
            );
            cloud_config.set_clean_session(false);
            cloud_config.set_keep_alive(aws_config.bridge.keepalive_interval.duration());

            let tls_config = tedge_config
                .mqtt_client_config_rustls(&aws_config)
                .context("Failed to create MQTT TLS config")?;
            cloud_config.set_transport(Transport::tls_with_config(tls_config.into()));

            configure_proxy(&tedge_config, &mut cloud_config)?;

            let bridge_name = format!("tedge-mapper-bridge-{prefix}");
            let health_topic = service_health_topic(&mqtt_schema, &device_topic_id, &bridge_name);

            let bridge_actor = MqttBridgeActorBuilder::new(
                &tedge_config,
                &bridge_name,
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
        let aws_converter = AwsConverter::new(
            aws_config.cloud_specific.mapper.timestamp,
            &mqtt_schema,
            aws_config.cloud_specific.mapper.timestamp_format,
            prefix.value().clone(),
            aws_config.mapper.mqtt.max_payload_size.0,
            aws_config.topics.to_string(),
        );
        let mapper_dir = self.mapper_dir(config_dir);
        let mut flows = crate::mapper_flow_registry(&tedge_config, mapper_dir).await?;
        aws_converter.persist_builtin_flow(&mut flows).await?;
        let service_config = flows_config(&tedge_config, &aws_mapper_name)?;

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
    let mapper_config_dir =
        tedge_config.mapper_config_dir::<AwsMapperSpecificConfig>(cloud_profile);
    let aws_reader = tedge_config.aws_reader(cloud_profile.map(|p| p.as_ref()))?;
    let mapper_table = crate::custom::resolve::reader_to_toml_table(aws_reader)?;
    let schema_json =
        serde_json::to_value(aws_reader).context("failed to serialise aws config to JSON")?;
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
        Some(profile) => format!("aws.{profile}"),
        None => "aws".to_string(),
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
    let mapper_config_dir =
        tedge_config.mapper_config_dir::<AwsMapperSpecificConfig>(cloud_profile);
    let config_root = tedge_config.config_root();
    if let Err(err) = config_root
        .dir(&mapper_config_dir)
        .context("invalid mapper config directory")?
        .ensure()
        .await
    {
        warn!("failed to set file ownership for '{mapper_config_dir}': {err}");
    }

    let bridge_config_dir = mapper_config_dir.join("bridge");

    persist_bridge_config_file(
        &bridge_config_dir,
        "rules",
        include_str!("bridge/rules.toml"),
        tedge_config,
    )
    .await?;

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
        let ttd = create_test_dir("aws.url = \"test.test.io\"").await;
        let (certificate, key) = make_self_signed_cert("test-device-id");
        let mapper_dir: camino::Utf8PathBuf = ttd.path().join("mappers/aws").try_into().unwrap();
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

        // Telemetry/command topics
        assert!(has_local_subscription(&rules, "aws/td/#"));
        assert!(has_remote_subscription(
            &rules,
            "thinedge/test-device-id/cmd/#"
        ));

        // Device shadow (bidirectional)
        assert!(has_local_subscription(&rules, "aws/shadow/#"));
        assert!(has_remote_subscription(
            &rules,
            "$aws/things/test-device-id/shadow/#"
        ));

        // Connection check
        assert!(has_local_subscription(&rules, "aws/test-connection"));
        assert!(has_remote_subscription(
            &rules,
            "thinedge/devices/test-device-id/test-connection"
        ));
    }

    #[tokio::test]
    async fn custom_topic_prefix_applied() {
        let ttd =
            create_test_dir("aws.url = \"test.test.io\"\naws.bridge.topic_prefix = \"custom-aws\"")
                .await;
        let (certificate, key) = make_self_signed_cert("test-device-id");
        let mapper_dir: camino::Utf8PathBuf = ttd.path().join("mappers/aws").try_into().unwrap();
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
                "device.cert_path = \"{mapper_dir}/cert.pem\"\ndevice.key_path = \"{mapper_dir}/key.pem\"\nbridge.topic_prefix = \"custom-aws\"\n",
            ),
        )
        .await
        .unwrap();
        let config = TEdgeConfig::load(ttd.path()).await.unwrap();

        let rules = bridge_rules(&config, None).await.unwrap();

        assert!(has_local_subscription(&rules, "custom-aws/td/#"));
        assert!(has_local_subscription(&rules, "custom-aws/shadow/#"));
    }

    async fn create_test_dir(toml: &str) -> TempTedgeDir {
        let ttd = TempTedgeDir::new();
        let (user, group) = crate::test_helpers::current_user_group();
        ttd.file("system.toml")
            .with_raw_content(&format!("user = '{user}'\ngroup = '{group}'\n"));
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
