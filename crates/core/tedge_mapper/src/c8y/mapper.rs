use crate::core::component::TEdgeComponent;
use crate::core::mapper::start_basic_actors;
use crate::core::mqtt::configure_proxy;
use crate::core::mqtt::flows_status_topic;
use anyhow::Context;
use async_trait::async_trait;
use c8y_api::http_proxy::read_c8y_credentials;
use c8y_auth_proxy::actor::C8yAuthProxyBuilder;
use c8y_mapper_ext::actor::C8yMapperBuilder;
use c8y_mapper_ext::availability::AvailabilityBuilder;
use c8y_mapper_ext::availability::AvailabilityConfig;
use c8y_mapper_ext::compatibility_adapter::OldAgentAdapter;
use c8y_mapper_ext::config::C8yMapperConfig;
use c8y_mapper_ext::converter::CumulocityConverter;
use mqtt_channel::Config;
use tedge_api::entity::EntityExternalId;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_config::models::MQTT_CORE_TLS_PORT;
use tedge_config::models::MQTT_SERVICE_TLS_PORT;
use tedge_config::tedge_toml::mapper_config;
use tedge_config::tedge_toml::mapper_config::C8yMapperSpecificConfig;
use tedge_config::tedge_toml::ProfileName;
use tedge_config::TEdgeConfig;
use tedge_downloader_ext::DownloaderActor;
use tedge_file_system_ext::FsWatchActorBuilder;
use tedge_flows::FlowsMapperBuilder;
use tedge_http_ext::HttpActor;
use tedge_mqtt_bridge::load_bridge_rules_from_directory;
use tedge_mqtt_bridge::persist_bridge_config_file;
use tedge_mqtt_bridge::rumqttc::LastWill;
use tedge_mqtt_bridge::rumqttc::Publish;
use tedge_mqtt_bridge::rumqttc::Transport;
use tedge_mqtt_bridge::use_credentials;
use tedge_mqtt_bridge::AuthMethod;
use tedge_mqtt_bridge::BridgeConfig;
use tedge_mqtt_bridge::MqttBridgeActorBuilder;
use tedge_mqtt_bridge::MqttOptions;
use tedge_mqtt_bridge::QoS;
use tedge_mqtt_ext::MqttActorBuilder;
use tedge_timer_ext::TimerActor;
use tedge_uploader_ext::UploaderActor;
use tedge_utils::file::change_mode;
use tedge_utils::file::change_user_and_group;
use tedge_watch_ext::WatchActorBuilder;
use tracing::warn;
use yansi::Paint;

pub struct CumulocityMapper {
    pub profile: Option<ProfileName>,
}

#[async_trait]
impl TEdgeComponent for CumulocityMapper {
    async fn start(
        &self,
        tedge_config: TEdgeConfig,
        cfg_dir: &tedge_config::Path,
    ) -> Result<(), anyhow::Error> {
        let c8y_config = tedge_config.mapper_config(&self.profile)?;
        let prefix = &c8y_config.bridge.topic_prefix;
        let c8y_mapper_name = format!("tedge-mapper-{prefix}");
        let (mut runtime, mut mqtt_actor) =
            start_basic_actors(&c8y_mapper_name, &tedge_config).await?;
        let mqtt_schema = MqttSchema::with_root(tedge_config.mqtt.topic_root.clone());
        let service_topic_id = EntityTopicId::default_main_service(&c8y_mapper_name)?;

        let c8y_mapper_config =
            C8yMapperConfig::from_tedge_config(cfg_dir, &tedge_config, &c8y_config)?;
        if tedge_config.mqtt.bridge.built_in {
            let (tc, cloud_config, reconnect_message_mapper) = mqtt_bridge_config(
                &tedge_config,
                &c8y_config,
                &c8y_mapper_config,
                &c8y_mapper_name,
                self.profile.as_ref(),
            )
            .await?;
            runtime
                .spawn(
                    MqttBridgeActorBuilder::new(
                        &tedge_config,
                        &c8y_mapper_config.bridge_service_name,
                        &c8y_mapper_config.bridge_health_topic,
                        tc,
                        cloud_config,
                        Some(reconnect_message_mapper),
                    )
                    .await,
                )
                .await?;
        } else if tedge_config.proxy.address.or_none().is_some() {
            warn!("`proxy.address` is configured without the built-in bridge enabled. The bridge MQTT connection to the cloud will {} communicate via the configured proxy.", "not".bold())
        }

        let mut http_actor = HttpActor::new(tedge_config.http.client_tls_config()?).builder();
        let c8y_auth_proxy_actor =
            C8yAuthProxyBuilder::try_from_config(&tedge_config, &c8y_config).await?;

        let mut fs_watch_actor = FsWatchActorBuilder::new();
        let mut cmd_watcher_actor = WatchActorBuilder::new();
        let mut timer_actor = TimerActor::builder();

        let identity = tedge_config.http.client.auth.identity()?;
        let cloud_root_certs = tedge_config.cloud_root_certs().await?;
        let mut uploader_actor =
            UploaderActor::new(identity.clone(), cloud_root_certs.clone()).builder();
        let mut downloader_actor = DownloaderActor::new(identity, cloud_root_certs).builder();

        // MQTT client dedicated to monitor the c8y-bridge client status and also
        // set service down status on shutdown, using a last-will message.
        // A separate MQTT actor/client is required as the last will message of the main MQTT actor
        // is used to send down status to health topic.
        let mut service_monitor_actor = MqttActorBuilder::new(service_monitor_client_config(
            &c8y_mapper_name,
            &tedge_config,
            &c8y_config,
        )?);

        C8yMapperBuilder::init(&c8y_mapper_config).await?;
        let mut c8y_mapper_actor = C8yMapperBuilder::try_new(
            c8y_mapper_config,
            &mut mqtt_actor,
            &mut http_actor,
            &mut timer_actor,
            &mut uploader_actor,
            &mut downloader_actor,
            &mut fs_watch_actor,
            &mut service_monitor_actor,
        )?;

        let c8y_prefix = &c8y_config.bridge.topic_prefix;
        // Adaptor translating commands sent on te/device/main///cmd/+/+ into requests on tedge/commands/req/+/+
        // and translating the responses received on tedge/commands/res/+/+ to te/device/main///cmd/+/+
        let old_to_new_agent_adapter = OldAgentAdapter::builder(c8y_prefix, &mut mqtt_actor);

        let availability_actor = if c8y_config.cloud_specific.availability.enable {
            Some(AvailabilityBuilder::new(
                AvailabilityConfig::try_new(&tedge_config, &c8y_config)?,
                &mut c8y_mapper_actor,
                &mut timer_actor,
            ))
        } else {
            None
        };

        let flows_dir =
            tedge_flows::flows_dir(cfg_dir, "c8y", self.profile.as_ref().map(|p| p.as_ref()));
        let flows = c8y_mapper_actor.flow_registry(flows_dir).await?;
        let flows_status = flows_status_topic(&mqtt_schema, &service_topic_id);

        let mut flows_mapper = FlowsMapperBuilder::try_new(flows, flows_status).await?;
        flows_mapper.connect(&mut mqtt_actor);
        flows_mapper.connect_fs(&mut fs_watch_actor);
        flows_mapper.connect_cmd(&mut cmd_watcher_actor);
        c8y_mapper_actor.set_flow_context(flows_mapper.context_handle());

        runtime.spawn(flows_mapper).await?;
        runtime.spawn(cmd_watcher_actor).await?;
        runtime.spawn(mqtt_actor).await?;
        runtime.spawn(http_actor).await?;
        runtime.spawn(c8y_auth_proxy_actor).await?;
        runtime.spawn(fs_watch_actor).await?;
        runtime.spawn(timer_actor).await?;
        runtime.spawn(c8y_mapper_actor).await?;
        runtime.spawn(service_monitor_actor).await?;
        runtime.spawn(uploader_actor).await?;
        runtime.spawn(downloader_actor).await?;
        runtime.spawn(old_to_new_agent_adapter).await?;
        if let Some(availability_actor) = availability_actor {
            runtime.spawn(availability_actor).await?;
        }
        runtime.run_to_completion().await?;

        Ok(())
    }
}

pub fn service_monitor_client_config(
    c8y_mapper_name: &str,
    tedge_config: &TEdgeConfig,
    c8y_config: &mapper_config::C8yMapperConfig,
) -> Result<Config, anyhow::Error> {
    let main_device_xid: EntityExternalId = c8y_config.device.id()?.into();
    let service_type = &tedge_config.service.ty;
    let service_type = if service_type.is_empty() {
        "service".to_string()
    } else {
        service_type.to_string()
    };

    // FIXME: this will not work if `mqtt.device_topic_id` is not in default scheme

    // there is one mapper instance per cloud per thin-edge instance, perhaps we should use some
    // predefined topic id instead of trying to derive it from current device?
    let entity_topic_id: EntityTopicId = tedge_config.mqtt.device_topic_id.clone();
    let prefix = &c8y_config.bridge.topic_prefix;

    let mapper_service_topic_id = entity_topic_id
        .default_service_for_device(c8y_mapper_name)
        .context("Can't derive service name if device topic id not in default scheme")?;

    let mapper_service_external_id =
        CumulocityConverter::map_to_c8y_external_id(&mapper_service_topic_id, &main_device_xid);

    let last_will_message = c8y_api::smartrest::inventory::service_creation_message(
        mapper_service_external_id.as_ref(),
        c8y_mapper_name,
        service_type.as_str(),
        "down",
        None,
        main_device_xid.as_ref(),
        prefix,
    )?;

    let mqtt_config = tedge_config
        .mqtt_config()?
        .with_session_name(format!("last_will_{prefix}_mapper"))
        .with_last_will_message(last_will_message);
    Ok(mqtt_config)
}

async fn mqtt_bridge_config(
    tedge_config: &TEdgeConfig,
    c8y_config: &mapper_config::C8yMapperConfig,
    c8y_mapper_config: &C8yMapperConfig,
    c8y_mapper_name: &str,
    cloud_profile: Option<&ProfileName>,
) -> Result<(BridgeConfig, MqttOptions, Publish), anyhow::Error> {
    let use_certificate = c8y_config
        .cloud_specific
        .auth_method
        .is_certificate(&c8y_config.cloud_specific.credentials_path);
    let use_mqtt_service = c8y_config.cloud_specific.mqtt_service.enabled;
    let bridge_config = bridge_rules(tedge_config, cloud_profile).await?;

    let c8y = &c8y_config.mqtt().or_config_not_set()?;
    let mut c8y_port = c8y.port().into();
    if use_mqtt_service && c8y_port == MQTT_CORE_TLS_PORT {
        c8y_port = MQTT_SERVICE_TLS_PORT;
    }
    let mut cloud_config = tedge_mqtt_bridge::MqttOptions::new(
        c8y_config.device.id()?,
        c8y.host().to_string(),
        c8y_port,
    );
    // Cumulocity tells us not to not set clean session to false, so don't
    // https://cumulocity.com/docs/device-integration/mqtt/#mqtt-clean-session
    cloud_config.set_clean_session(true);

    if use_certificate {
        let tls_config = tedge_config
            .mqtt_client_config_rustls(c8y_config)
            .context("Failed to create MQTT TLS config")?;
        cloud_config.set_transport(Transport::tls_with_config(tls_config.into()));
    } else {
        // TODO(marcel): integrate credentials auth into MqttAuthConfig?
        let (username, password) =
            read_c8y_credentials(&c8y_config.cloud_specific.credentials_path)?;
        use_credentials(
            &mut cloud_config,
            &*c8y_config.root_cert_path,
            username,
            password,
        )?;
    }

    let main_device_xid: EntityExternalId = c8y_config.device.id()?.into();
    let service_type = &tedge_config.service.ty;
    let service_type = if service_type.is_empty() {
        "service".to_string()
    } else {
        service_type.to_string()
    };

    // FIXME: this will not work if `mqtt.device_topic_id` is not in default scheme

    // there is one mapper instance per cloud per thin-edge instance, perhaps we should use some
    // predefined topic id instead of trying to derive it from current device?
    let entity_topic_id: EntityTopicId = tedge_config.mqtt.device_topic_id.clone();

    let mapper_service_topic_id = entity_topic_id
        .default_service_for_device(c8y_mapper_name)
        .context("Can't derive service name if device topic id not in default scheme")?;

    let mapper_service_external_id =
        CumulocityConverter::map_to_c8y_external_id(&mapper_service_topic_id, &main_device_xid);

    let last_will_message_mapper = c8y_api::smartrest::inventory::service_creation_message_payload(
        mapper_service_external_id.as_ref(),
        c8y_mapper_name,
        service_type.as_str(),
        "down",
    )?
    .into_inner();
    let last_will_message_bridge = c8y_api::smartrest::inventory::service_creation_message_payload(
        mapper_service_external_id.as_ref(),
        &c8y_mapper_config.bridge_service_name,
        service_type.as_str(),
        "down",
    )?
    .into_inner();

    let reconnect_message_mapper = c8y_api::smartrest::inventory::service_creation_message_payload(
        mapper_service_external_id.as_ref(),
        c8y_mapper_name,
        service_type.as_str(),
        "up",
    )?
    .into_inner();
    let reconnect_message_mapper = Publish {
        topic: "s/us".into(),
        qos: QoS::AtLeastOnce,
        payload: reconnect_message_mapper.into(),
        retain: false,
        dup: false,
        pkid: 0,
    };

    cloud_config.set_last_will(LastWill {
        topic: "s/us".into(),
        qos: QoS::AtLeastOnce,
        message: format!("{last_will_message_bridge}\n{last_will_message_mapper}").into(),
        retain: false,
    });
    cloud_config.set_keep_alive(c8y_config.bridge.keepalive_interval.duration());

    configure_proxy(tedge_config, &mut cloud_config)?;
    Ok((bridge_config, cloud_config, reconnect_message_mapper))
}

pub async fn bridge_rules(
    tedge_config: &TEdgeConfig,
    cloud_profile: Option<&ProfileName>,
) -> anyhow::Result<BridgeConfig> {
    let bridge_config_dir = tedge_config
        .mapper_config_dir::<C8yMapperSpecificConfig>(cloud_profile)
        .join("bridge");

    // Persist the built-in bridge configuration templates
    persist_bridge_config_file(
        &bridge_config_dir,
        "mqtt-core",
        include_str!("bridge/mqtt-core.toml"),
    )
    .await?;

    if let Err(err) = change_user_and_group(&bridge_config_dir, "tedge", "tedge").await {
        warn!("failed to set file ownership for '{bridge_config_dir}': {err}");
    }

    if let Err(err) = change_mode(&bridge_config_dir, 0o755).await {
        warn!("failed to set file permissions for '{bridge_config_dir}': {err}");
    }

    let c8y_config = tedge_config.c8y_mapper_config(&cloud_profile)?;
    let use_certificate = c8y_config
        .cloud_specific
        .auth_method
        .is_certificate(&c8y_config.cloud_specific.credentials_path);

    let auth_method = if use_certificate {
        AuthMethod::Certificate
    } else {
        AuthMethod::Password
    };

    load_bridge_rules_from_directory(&bridge_config_dir, tedge_config, auth_method, cloud_profile)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tedge_test_utils::fs::TempTedgeDir;

    async fn load_config(toml: &str) -> (TempTedgeDir, TEdgeConfig) {
        let ttd = TempTedgeDir::new();
        ttd.file("tedge.toml").with_raw_content(toml);
        let config = TEdgeConfig::load(ttd.path()).await.unwrap();
        (ttd, config)
    }

    mod template_rules {
        use super::*;

        fn has_local_subscription(config: &BridgeConfig, topic: &str) -> bool {
            config.local_subscriptions().any(|t| t == topic)
        }

        fn has_remote_subscription(config: &BridgeConfig, topic: &str) -> bool {
            config.remote_subscriptions().any(|t| t == topic)
        }

        #[tokio::test]
        async fn certificate_only_rules_present_with_certificate_auth() {
            let (_ttd, config) = load_config(
                r#"
[c8y]
url = "example.com"
"#,
            )
            .await;
            let rules = bridge_rules(&config, None).await.unwrap();

            assert!(has_remote_subscription(&rules, "s/dat"));
            assert!(has_local_subscription(&rules, "c8y/s/uat"));
        }

        #[tokio::test]
        async fn certificate_only_rules_absent_with_password_auth() {
            let (_ttd, config) = load_config(
                r#"
[c8y]
url = "example.com"
auth_method = "basic"
"#,
            )
            .await;
            let rules = bridge_rules(&config, None).await.unwrap();

            assert!(!has_remote_subscription(&rules, "s/dat"));
            assert!(!has_local_subscription(&rules, "c8y/s/uat"));
        }

        #[tokio::test]
        async fn password_only_rules_present_with_password_auth() {
            let (_ttd, config) = load_config(
                r#"
[c8y]
url = "example.com"
auth_method = "basic"
"#,
            )
            .await;
            let rules = bridge_rules(&config, None).await.unwrap();

            for mode in ['s', 't', 'q', 'c'] {
                assert!(has_local_subscription(&rules, &format!("c8y/{mode}/ul/#")));
            }
        }

        #[tokio::test]
        async fn password_only_rules_absent_with_certificate_auth() {
            let (_ttd, config) = load_config(
                r#"
[c8y]
url = "example.com"
"#,
            )
            .await;
            let rules = bridge_rules(&config, None).await.unwrap();

            for mode in ['s', 't', 'q', 'c'] {
                assert!(!has_local_subscription(&rules, &format!("c8y/{mode}/ul/#")));
            }
        }

        #[tokio::test]
        async fn smartrest1_templates_expand_correctly() {
            let (_ttd, config) = load_config(
                r#"
[c8y]
url = "example.com"
smartrest1.templates = ["tmpl1", "tmpl2"]
"#,
            )
            .await;
            let rules = bridge_rules(&config, None).await.unwrap();

            assert!(has_remote_subscription(&rules, "s/dl/tmpl1"));
            assert!(has_remote_subscription(&rules, "s/dl/tmpl2"));
        }

        #[tokio::test]
        async fn smartrest2_templates_expand_correctly() {
            let (_ttd, config) = load_config(
                r#"
[c8y]
url = "example.com"
smartrest.templates = ["sr2a", "sr2b"]
"#,
            )
            .await;
            let rules = bridge_rules(&config, None).await.unwrap();

            assert!(has_remote_subscription(&rules, "s/dc/sr2a"));
            assert!(has_remote_subscription(&rules, "s/dc/sr2b"));
        }

        #[tokio::test]
        async fn mode_loop_expands_correctly() {
            let (_ttd, config) = load_config(
                r#"
[c8y]
url = "example.com"
"#,
            )
            .await;
            let rules = bridge_rules(&config, None).await.unwrap();

            for mode in ['s', 't', 'q', 'c'] {
                assert!(has_local_subscription(&rules, &format!("c8y/{mode}/us/#")));
                assert!(has_local_subscription(&rules, &format!("c8y/{mode}/uc/#")));
            }
        }

        #[tokio::test]
        async fn mqtt_service_rules_when_enabled() {
            let (_ttd, config) = load_config(
                r#"
[c8y]
url = "example.com"
mqtt_service.enabled = true
mqtt_service.topics = ["custom/topic"]
"#,
            )
            .await;
            let rules = bridge_rules(&config, None).await.unwrap();

            assert!(has_local_subscription(&rules, "c8y/mqtt/out/#"));
            assert!(has_remote_subscription(&rules, "custom/topic"));
        }

        #[tokio::test]
        async fn mqtt_service_rules_absent_when_disabled() {
            let (_ttd, config) = load_config(
                r#"
[c8y]
url = "example.com"
"#,
            )
            .await;
            let rules = bridge_rules(&config, None).await.unwrap();

            assert!(!has_local_subscription(&rules, "c8y/mqtt/out/#"));
        }

        #[tokio::test]
        async fn custom_topic_prefix_applied() {
            let (_ttd, config) = load_config(
                r#"
[c8y]
url = "example.com"
bridge.topic_prefix = "custom"
"#,
            )
            .await;
            let rules = bridge_rules(&config, None).await.unwrap();

            assert!(has_remote_subscription(&rules, "s/dt"));
            assert!(has_local_subscription(&rules, "custom/s/us/#"));
        }
    }
}
