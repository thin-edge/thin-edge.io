use crate::core::component::TEdgeComponent;
use crate::core::mapper::start_basic_actors;
use crate::core::mqtt::configure_proxy;
use crate::flows_config;
use anyhow::Context;
use async_trait::async_trait;
use c8y_api::http_proxy::read_c8y_credentials;
use c8y_auth_proxy::actor::C8yAuthProxyBuilder;
use c8y_mapper_ext::actor::C8yMapperBuilder;
use c8y_mapper_ext::availability::AvailabilityBuilder;
use c8y_mapper_ext::availability::AvailabilityConfig;
use c8y_mapper_ext::config::C8yMapperConfig;
use c8y_mapper_ext::converter::CumulocityConverter;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use mqtt_channel::Config;
use tedge_api::entity::EntityExternalId;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_config::models::MQTT_CORE_TLS_PORT;
use tedge_config::models::MQTT_SERVICE_TLS_PORT;
use tedge_config::tedge_toml::mapper_config;
use tedge_config::tedge_toml::mapper_config::C8yMapperSpecificConfig;
use tedge_config::tedge_toml::mapper_config::MapperConfig;
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
use tedge_watch_ext::WatchActorBuilder;
use tracing::warn;
use yansi::Paint;

pub struct CumulocityMapper {
    pub profile: Option<ProfileName>,
}

pub fn auth_method(c8y_config: &MapperConfig<C8yMapperSpecificConfig>) -> AuthMethod {
    let use_certificate = c8y_config
        .cloud_specific
        .auth_method
        .is_certificate(&c8y_config.cloud_specific.credentials_path);

    if use_certificate {
        AuthMethod::Certificate
    } else {
        AuthMethod::Password
    }
}

impl CumulocityMapper {
    /// Returns the mapper directory path for this instance.
    pub fn mapper_dir(&self, config_dir: &Utf8Path) -> Utf8PathBuf {
        crate::mapper_dir(config_dir, "c8y", self.profile.as_ref())
    }
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
        let service_topic_id = EntityTopicId::default_main_service(&c8y_mapper_name)?;

        let c8y_mapper_config = C8yMapperConfig::from_tedge_config(
            cfg_dir,
            &tedge_config,
            &c8y_config,
            service_topic_id.clone(),
        )?;
        let auth_method = auth_method(&c8y_config);
        if tedge_config.mqtt.bridge.built_in {
            let (tc, cloud_config, reconnect_message_mapper) = mqtt_bridge_config(
                &tedge_config,
                &c8y_config,
                &c8y_mapper_config,
                &c8y_mapper_name,
                self.profile.as_ref(),
                auth_method,
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
            &mut uploader_actor,
            &mut downloader_actor,
            &mut fs_watch_actor,
            &mut service_monitor_actor,
        )?;

        let availability_actor = if c8y_config.cloud_specific.availability.enable {
            Some(AvailabilityBuilder::new(
                AvailabilityConfig::try_new(&tedge_config, &c8y_config)?,
                &mut c8y_mapper_actor,
                &mut timer_actor,
            ))
        } else {
            None
        };

        let mapper_dir = self.mapper_dir(cfg_dir);
        let mut flows = crate::mapper_flow_registry(&tedge_config, mapper_dir).await?;
        c8y_mapper_actor.persist_builtin_flows(&mut flows).await?;
        let service_config = flows_config(&tedge_config, &c8y_mapper_name)?;

        let mut flows_mapper = FlowsMapperBuilder::try_new(flows, service_config).await?;
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
    auth_method: AuthMethod,
) -> Result<(BridgeConfig, MqttOptions, Publish), anyhow::Error> {
    let use_mqtt_service = c8y_config.cloud_specific.mqtt_service.enabled;
    let bridge_config = bridge_rules(tedge_config, cloud_profile, auth_method).await?;

    let c8y = &c8y_config.mqtt().or_config_not_set()?;
    let mut c8y_port = c8y.port().into();
    if use_mqtt_service && c8y_port == MQTT_CORE_TLS_PORT {
        c8y_port = MQTT_SERVICE_TLS_PORT;
    }
    let mut cloud_config = tedge_mqtt_bridge::MqttOptions::new(
        c8y_config.device.id()?,
        mqtt_channel::Broker::tcp(c8y.host().to_string(), c8y_port),
    );
    // Cumulocity tells us not to not set clean session to false, so don't
    // https://cumulocity.com/docs/device-integration/mqtt/#mqtt-clean-session
    cloud_config.set_clean_session(true);

    match auth_method {
        AuthMethod::Certificate => {
            let tls_config = tedge_config
                .mqtt_client_config_rustls(c8y_config)
                .context("Failed to create MQTT TLS config")?;
            cloud_config.set_transport(Transport::tls_with_config(tls_config.into()));
        }
        AuthMethod::Password => {
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
    cloud_config.set_keep_alive(c8y_config.bridge.keepalive_interval.duration().as_secs() as u16);

    configure_proxy(tedge_config, &mut cloud_config)?;
    Ok((bridge_config, cloud_config, reconnect_message_mapper))
}

/// Returns the effective mapper config for the c8y built-in mapper.
///
/// Serialises the full C8y reader (with all defaults applied) as the TOML overlay and
/// JSON schema, then merges any user `mapper.toml` overrides. This is the same config
/// path that [`bridge_rules`] uses at runtime, so CLI commands always reflect the real
/// values.
pub async fn resolve_effective_mapper_config(
    tedge_config: &TEdgeConfig,
    cloud_profile: Option<&ProfileName>,
) -> anyhow::Result<crate::custom::resolve::EffectiveMapperConfig> {
    let mapper_config_dir =
        tedge_config.mapper_config_dir::<C8yMapperSpecificConfig>(cloud_profile);
    // Serialize the full c8y reader (defaults applied) as the base mapper config so that
    // built-in bridge files (e.g. mqtt-core.toml) can use ${mapper.*} template references
    // for any c8y config key without manual enumeration.
    let c8y_reader = tedge_config.c8y_reader(cloud_profile.map(|p| p.as_ref()))?;
    let mapper_table = crate::custom::resolve::reader_to_toml_table(c8y_reader)?;
    // JSON serialisation preserves OptionalConfig::Empty as null, giving a complete schema
    // that includes keys with no configured value. This lets get() distinguish NotSet from
    // UnknownKey for c8y-specific optional keys that are absent from the TOML overlay.
    let schema_json =
        serde_json::to_value(c8y_reader).context("failed to serialise c8y config to JSON")?;
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
        Some(profile) => format!("c8y.{profile}"),
        None => "c8y".to_string(),
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

pub async fn bridge_rules(
    tedge_config: &TEdgeConfig,
    cloud_profile: Option<&ProfileName>,
    auth_method: AuthMethod,
) -> anyhow::Result<BridgeConfig> {
    let mapper_config_dir =
        tedge_config.mapper_config_dir::<C8yMapperSpecificConfig>(cloud_profile);
    let config_root = tedge_config.config_root();

    if let Err(err) = config_root
        .dir(mapper_config_dir.join("bridge"))
        .context("invalid mapper config directory")?
        .ensure()
        .await
    {
        warn!("failed to set file ownership for '{mapper_config_dir}': {err}");
    }

    let bridge_config_dir = mapper_config_dir.join("bridge");

    // Persist the built-in bridge configuration templates
    persist_bridge_config_file(
        &bridge_config_dir,
        "mqtt-core",
        include_str!("bridge/mqtt-core.toml"),
        tedge_config,
    )
    .await?;

    let effective = resolve_effective_mapper_config(tedge_config, cloud_profile).await?;

    load_bridge_rules_from_directory(
        &bridge_config_dir,
        tedge_config,
        auth_method,
        cloud_profile,
        &effective,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tedge_test_utils::fs::TempTedgeDir;

    async fn load_config(toml: &str) -> (TempTedgeDir, TEdgeConfig) {
        let ttd = TempTedgeDir::new();
        let (user, group) = crate::test_helpers::current_user_group();
        ttd.file("system.toml")
            .with_raw_content(&format!("user = '{user}'\ngroup = '{group}'\n"));
        ttd.file("tedge.toml").with_raw_content(toml);
        let config = TEdgeConfig::load(ttd.path()).await.unwrap();
        (ttd, config)
    }

    mod template_rules {
        use camino::Utf8PathBuf;

        use super::*;

        fn has_local_subscription(config: &BridgeConfig, topic: &str) -> bool {
            config.local_subscriptions().any(|t| t == topic)
        }

        fn has_remote_subscription(config: &BridgeConfig, topic: &str) -> bool {
            config.remote_subscriptions().any(|t| t == topic)
        }

        #[tokio::test]
        async fn certificate_only_rules_present_with_certificate_auth() {
            let (_ttd, config) = load_config("").await;
            let rules = bridge_rules(&config, None, AuthMethod::Certificate)
                .await
                .unwrap();

            assert!(has_remote_subscription(&rules, "s/dat"));
            assert!(has_local_subscription(&rules, "c8y/s/uat"));
        }

        #[tokio::test]
        async fn certificate_only_rules_absent_with_password_auth() {
            let (_ttd, config) = load_config("").await;
            let rules = bridge_rules(&config, None, AuthMethod::Password)
                .await
                .unwrap();

            assert!(!has_remote_subscription(&rules, "s/dat"));
            assert!(!has_local_subscription(&rules, "c8y/s/uat"));
        }

        #[tokio::test]
        async fn password_only_rules_present_with_password_auth() {
            let (_ttd, config) = load_config(r#"c8y.auth_method = "basic""#).await;
            let rules = bridge_rules(&config, None, AuthMethod::Password)
                .await
                .unwrap();

            for mode in ['s', 't', 'q', 'c'] {
                assert!(has_local_subscription(&rules, &format!("c8y/{mode}/ul/#")));
            }
        }

        #[tokio::test]
        async fn password_only_rules_absent_with_certificate_auth() {
            let (_ttd, config) = load_config("").await;
            let rules = bridge_rules(&config, None, AuthMethod::Certificate)
                .await
                .unwrap();

            for mode in ['s', 't', 'q', 'c'] {
                assert!(!has_local_subscription(&rules, &format!("c8y/{mode}/ul/#")));
            }
        }

        #[tokio::test]
        async fn smartrest1_templates_expand_correctly() {
            let (_ttd, config) =
                load_config(r#"c8y.smartrest1.templates = ["tmpl1", "tmpl2"]"#).await;
            let rules = bridge_rules(&config, None, AuthMethod::Password)
                .await
                .unwrap();

            assert!(has_remote_subscription(&rules, "s/dl/tmpl1"));
            assert!(has_remote_subscription(&rules, "s/ol/tmpl1"));
            assert!(has_remote_subscription(&rules, "s/dl/tmpl2"));
            assert!(has_remote_subscription(&rules, "s/ol/tmpl2"));
        }

        #[tokio::test]
        async fn smartrest2_templates_expand_correctly() {
            let (_ttd, config) = load_config(r#"c8y.smartrest.templates = ["sr2a", "sr2b"]"#).await;
            let rules = bridge_rules(&config, None, AuthMethod::Certificate)
                .await
                .unwrap();

            assert!(has_remote_subscription(&rules, "s/dc/sr2a"));
            assert!(has_remote_subscription(&rules, "s/dc/sr2b"));
        }

        #[tokio::test]
        async fn mode_loop_expands_correctly() {
            let (_ttd, config) = load_config("").await;
            let rules = bridge_rules(&config, None, AuthMethod::Certificate)
                .await
                .unwrap();

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
mqtt_service.enabled = true
mqtt_service.topics = ["custom/topic"]
"#,
            )
            .await;
            let rules = bridge_rules(&config, None, AuthMethod::Certificate)
                .await
                .unwrap();

            assert!(has_local_subscription(&rules, "c8y/mqtt/out/#"));
            assert!(has_remote_subscription(&rules, "custom/topic"));
        }

        #[tokio::test]
        async fn mqtt_service_rules_absent_when_disabled() {
            let (_ttd, config) = load_config("").await;
            let rules = bridge_rules(&config, None, AuthMethod::Certificate)
                .await
                .unwrap();

            assert!(!has_local_subscription(&rules, "c8y/mqtt/out/#"));
        }

        #[tokio::test]
        async fn custom_topic_prefix_applied() {
            let (_ttd, config) = load_config("c8y.bridge.topic_prefix = \"custom\"").await;
            let rules = bridge_rules(&config, None, AuthMethod::Certificate)
                .await
                .unwrap();

            assert!(has_remote_subscription(&rules, "s/dt"));
            assert!(has_local_subscription(&rules, "custom/s/us/#"));
        }

        #[tokio::test]
        async fn mqtt_service_local_prefix_uses_custom_topic_prefix() {
            // Verifies that both the mqtt/out and mqtt/in local_prefix rules expand
            // ${mapper.bridge.topic_prefix} from the serialised C8y config, not the
            // old ${tedge.c8y.bridge.topic_prefix} literal.  Previously the mqtt/out
            // rule was left on the old notation during the ${mapper.*} migration.
            let (_ttd, config) = load_config(
                r#"
[c8y]
bridge.topic_prefix = "custom"
mqtt_service.enabled = true
mqtt_service.topics = ["custom/topic"]
"#,
            )
            .await;
            let rules = bridge_rules(&config, None, AuthMethod::Certificate)
                .await
                .unwrap();

            // Outbound: bridge subscribes locally to custom/mqtt/out/#
            assert!(has_local_subscription(&rules, "custom/mqtt/out/#"));
            // Inbound: bridge subscribes remotely to custom/topic (the for-loop item)
            // The local_prefix "custom/mqtt/in/" is where it publishes locally — not a subscription.
            assert!(has_remote_subscription(&rules, "custom/topic"));
        }

        #[tokio::test]
        async fn user_bridge_file_can_use_mapper_namespace() {
            let (ttd, _config) = load_config("").await;
            // Create a mapper.toml with a value ${mapper.*} can reference
            let mapper_dir = ttd.path().join("mappers/c8y");
            tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
            tokio::fs::write(
                mapper_dir.join("mapper.toml"),
                "device.id = \"test-device-id\"\n",
            )
            .await
            .unwrap();
            // Create a supplemental bridge file that uses ${mapper.url}
            let bridge_dir = mapper_dir.join("bridge");
            tokio::fs::create_dir_all(&bridge_dir).await.unwrap();
            tokio::fs::write(
                bridge_dir.join("custom.toml"),
                "local_prefix = \"\"\nremote_prefix = \"\"\n\n[[rule]]\ntopic = \"${mapper.device.id}/#\"\ndirection = \"inbound\"\n",
            )
            .await
            .unwrap();

            let config = TEdgeConfig::load(ttd.path()).await.unwrap();
            let rules = bridge_rules(&config, None, AuthMethod::Certificate)
                .await
                .unwrap();

            assert!(rules
                .remote_subscriptions()
                .any(|t| t == "test-device-id/#"));
        }

        #[tokio::test]
        async fn mapper_namespace_can_read_lazy_values() {
            let (ttd, _config) = load_config("").await;
            // Create a mapper.toml with a value ${mapper.*} can reference
            let mapper_dir: Utf8PathBuf = ttd.path().join("mappers/c8y").try_into().unwrap();
            let certificate = rcgen::generate_simple_self_signed(vec!["my-device".into()]).unwrap();
            tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
            tokio::fs::write(mapper_dir.join("cert.pem"), certificate.cert.pem())
                .await
                .unwrap();
            tokio::fs::write(
                mapper_dir.join("key.pem"),
                certificate.signing_key.serialize_pem(),
            )
            .await
            .unwrap();
            tokio::fs::write(
                mapper_dir.join("mapper.toml"),
                format!("device.cert_path = \"{mapper_dir}/cert.pem\"\ndevice.key_path = \"./key.pem\"\n"),
            )
            .await
            .unwrap();
            let config = TEdgeConfig::load(ttd.path()).await.unwrap();

            // Create a supplemental bridge file that uses ${mapper.device.id}
            let bridge_dir = mapper_dir.join("bridge");
            tokio::fs::create_dir_all(&bridge_dir).await.unwrap();
            tokio::fs::write(
                bridge_dir.join("mqtt-core.toml"),
                "local_prefix = \"\"\nremote_prefix = \"\"\n\n[[rule]]\ntopic = \"${mapper.device.id}/#\"\ndirection = \"inbound\"\n",
            )
            .await
            .unwrap();

            let rules = bridge_rules(&config, None, AuthMethod::Certificate)
                .await
                .unwrap();

            assert_eq!(
                rules.remote_subscriptions().collect::<Vec<_>>(),
                vec!["rcgen self signed cert/#"]
            );
        }
    }
}
