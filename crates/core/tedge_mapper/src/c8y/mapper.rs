use crate::core::component::TEdgeComponent;
use crate::core::mapper::start_basic_actors;
use crate::core::mqtt::configure_proxy;
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
use std::borrow::Cow;
use tedge_api::entity::EntityExternalId;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_config::tedge_toml::ProfileName;
use tedge_config::TEdgeConfig;
use tedge_downloader_ext::DownloaderActor;
use tedge_file_system_ext::FsWatchActorBuilder;
use tedge_http_ext::HttpActor;
use tedge_mqtt_bridge::rumqttc::LastWill;
use tedge_mqtt_bridge::rumqttc::Transport;
use tedge_mqtt_bridge::use_credentials;
use tedge_mqtt_bridge::BridgeConfig;
use tedge_mqtt_bridge::MqttBridgeActorBuilder;
use tedge_mqtt_bridge::QoS;
use tedge_mqtt_ext::MqttActorBuilder;
use tedge_timer_ext::TimerActor;
use tedge_uploader_ext::UploaderActor;

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
        let c8y_profile = self.profile.as_deref();
        let c8y_config = tedge_config.c8y.try_get(c8y_profile)?;
        let prefix = &c8y_config.bridge.topic_prefix;
        let c8y_mapper_name = format!("tedge-mapper-{prefix}");
        let (mut runtime, mut mqtt_actor) =
            start_basic_actors(&c8y_mapper_name, &tedge_config).await?;

        let c8y_mapper_config =
            C8yMapperConfig::from_tedge_config(cfg_dir, &tedge_config, c8y_profile)?;
        if tedge_config.mqtt.bridge.built_in {
            let smartrest_1_topics = c8y_config
                .smartrest1
                .templates
                .0
                .iter()
                .map(|id| Cow::Owned(format!("s/dl/{id}")));

            let smartrest_2_topics = c8y_config
                .smartrest
                .templates
                .0
                .iter()
                .map(|id| Cow::Owned(format!("s/dc/{id}")));

            let use_certificate = c8y_config
                .auth_method
                .is_certificate(&c8y_config.credentials_path);
            let cloud_topics = [
                ("s/dt", true),
                ("s/ds", true),
                ("s/dat", use_certificate),
                ("s/e", true),
                ("devicecontrol/notifications", true),
                ("error", true),
            ]
            .into_iter()
            .filter_map(|(topic, active)| {
                if active {
                    Some(Cow::Borrowed(topic))
                } else {
                    None
                }
            })
            .chain(smartrest_1_topics)
            .chain(smartrest_2_topics);

            let mut tc = BridgeConfig::new();
            let local_prefix = format!("{}/", c8y_config.bridge.topic_prefix.as_str());

            for topic in cloud_topics {
                tc.forward_from_remote(topic, local_prefix.clone(), "")?;
            }

            // Templates
            tc.forward_from_local("s/ut/#", local_prefix.clone(), "")?;

            // Static templates
            tc.forward_from_local("s/us/#", local_prefix.clone(), "")?;
            tc.forward_from_local("t/us/#", local_prefix.clone(), "")?;
            tc.forward_from_local("q/us/#", local_prefix.clone(), "")?;
            tc.forward_from_local("c/us/#", local_prefix.clone(), "")?;

            // SmartREST1
            if !use_certificate {
                tc.forward_from_local("s/ul/#", local_prefix.clone(), "")?;
                tc.forward_from_local("t/ul/#", local_prefix.clone(), "")?;
                tc.forward_from_local("q/ul/#", local_prefix.clone(), "")?;
                tc.forward_from_local("c/ul/#", local_prefix.clone(), "")?;
            }

            // SmartREST2
            tc.forward_from_local("s/uc/#", local_prefix.clone(), "")?;
            tc.forward_from_local("t/uc/#", local_prefix.clone(), "")?;
            tc.forward_from_local("q/uc/#", local_prefix.clone(), "")?;
            tc.forward_from_local("c/uc/#", local_prefix.clone(), "")?;

            // c8y JSON
            tc.forward_from_local(
                "inventory/managedObjects/update/#",
                local_prefix.clone(),
                "",
            )?;
            tc.forward_from_local(
                "measurement/measurements/create/#",
                local_prefix.clone(),
                "",
            )?;
            tc.forward_from_local(
                "measurement/measurements/createBulk/#",
                local_prefix.clone(),
                "",
            )?;
            tc.forward_from_local("event/events/create/#", local_prefix.clone(), "")?;
            tc.forward_from_local("event/events/createBulk/#", local_prefix.clone(), "")?;
            tc.forward_from_local("alarm/alarms/create/#", local_prefix.clone(), "")?;
            tc.forward_from_local("alarm/alarms/createBulk/#", local_prefix.clone(), "")?;

            // JWT token
            if use_certificate {
                tc.forward_from_local("s/uat", local_prefix.clone(), "")?;
            }

            let c8y = c8y_config.mqtt.or_config_not_set()?;
            let mut cloud_config = tedge_mqtt_bridge::MqttOptions::new(
                c8y_config.device.id()?,
                c8y.host().to_string(),
                c8y.port().into(),
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
                let (username, password) = read_c8y_credentials(&c8y_config.credentials_path)?;
                use_credentials(
                    &mut cloud_config,
                    &c8y_config.root_cert_path,
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
            let entity_topic_id: EntityTopicId = tedge_config
                .mqtt
                .device_topic_id
                .clone()
                .parse()
                .context("Invalid device_topic_id")?;

            let mapper_service_topic_id = entity_topic_id
                .default_service_for_device(&c8y_mapper_name)
                .context("Can't derive service name if device topic id not in default scheme")?;

            let mapper_service_external_id = CumulocityConverter::map_to_c8y_external_id(
                &mapper_service_topic_id,
                &main_device_xid,
            );

            let last_will_message_mapper =
                c8y_api::smartrest::inventory::service_creation_message_payload(
                    mapper_service_external_id.as_ref(),
                    &c8y_mapper_name,
                    service_type.as_str(),
                    "down",
                )?
                .into_inner();
            let last_will_message_bridge =
                c8y_api::smartrest::inventory::service_creation_message_payload(
                    mapper_service_external_id.as_ref(),
                    &c8y_mapper_config.bridge_service_name,
                    service_type.as_str(),
                    "down",
                )?
                .into_inner();

            cloud_config.set_last_will(LastWill {
                topic: "s/us".into(),
                qos: QoS::AtLeastOnce,
                message: format!("{last_will_message_bridge}\n{last_will_message_mapper}").into(),
                retain: false,
            });
            cloud_config.set_keep_alive(c8y_config.bridge.keepalive_interval.duration());

            configure_proxy(&tedge_config, &mut cloud_config)?;

            runtime
                .spawn(
                    MqttBridgeActorBuilder::new(
                        &tedge_config,
                        &c8y_mapper_config.bridge_service_name,
                        &c8y_mapper_config.bridge_health_topic,
                        tc,
                        cloud_config,
                    )
                    .await,
                )
                .await?;
        }

        let mut http_actor = HttpActor::new(tedge_config.http.client_tls_config()?).builder();
        let c8y_auth_proxy_actor =
            C8yAuthProxyBuilder::try_from_config(&tedge_config, c8y_profile)?;

        let mut fs_watch_actor = FsWatchActorBuilder::new();
        let mut timer_actor = TimerActor::builder();

        let identity = tedge_config.http.client.auth.identity()?;
        let cloud_root_certs = tedge_config.cloud_root_certs()?;
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
            c8y_profile,
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

        let availability_actor = if c8y_config.availability.enable {
            Some(AvailabilityBuilder::new(
                AvailabilityConfig::try_new(&tedge_config, c8y_profile)?,
                &mut c8y_mapper_actor,
                &mut timer_actor,
            ))
        } else {
            None
        };

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
    c8y_profile: Option<&str>,
) -> Result<Config, anyhow::Error> {
    let c8y_config = tedge_config.c8y.try_get(c8y_profile)?;
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
    let entity_topic_id: EntityTopicId = tedge_config
        .mqtt
        .device_topic_id
        .clone()
        .parse()
        .context("Invalid device_topic_id")?;
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
