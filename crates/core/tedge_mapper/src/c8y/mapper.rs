use crate::core::component::TEdgeComponent;
use crate::core::mapper::start_basic_actors;
use anyhow::Context;
use async_trait::async_trait;
use c8y_auth_proxy::actor::C8yAuthProxyBuilder;
use c8y_http_proxy::credentials::C8YJwtRetriever;
use c8y_http_proxy::C8YHttpProxyBuilder;
use c8y_mapper_ext::actor::C8yMapperBuilder;
use c8y_mapper_ext::compatibility_adapter::OldAgentAdapter;
use c8y_mapper_ext::config::C8yMapperConfig;
use c8y_mapper_ext::converter::CumulocityConverter;
use mqtt_channel::Config;
use std::path::Path;
use tedge_api::entity_store::EntityExternalId;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_config::TEdgeConfig;
use tedge_downloader_ext::DownloaderActor;
use tedge_file_system_ext::FsWatchActorBuilder;
use tedge_http_ext::HttpActor;
use tedge_mqtt_bridge::MqttBridgeActorBuilder;
use tedge_mqtt_ext::MqttActorBuilder;
use tedge_timer_ext::TimerActor;
use tedge_uploader_ext::UploaderActor;

const CUMULOCITY_MAPPER_NAME: &str = "tedge-mapper-c8y";

pub struct CumulocityMapper;

#[async_trait]
impl TEdgeComponent for CumulocityMapper {
    fn session_name(&self) -> &str {
        CUMULOCITY_MAPPER_NAME
    }

    async fn start(&self, tedge_config: TEdgeConfig, cfg_dir: &Path) -> Result<(), anyhow::Error> {
        let (mut runtime, mut mqtt_actor) =
            start_basic_actors(self.session_name(), &tedge_config).await?;

        let mqtt_config = tedge_config.mqtt_config()?;
        let custom_topics = tedge_config
            .c8y
            .smartrest
            .templates
            .0
            .iter()
            .map(|id| format!("s/dc/{id}"));
        let smartrest_topics: Vec<String> = [
            "s/dt",
            "s/dat",
            "s/ds",
            "s/e",
            "s/dc/#",
            "devicecontrol/notifications",
            "error",
        ]
        .into_iter()
        .map(<_>::to_owned)
        .chain(custom_topics)
        .collect();
        let bridge_actor = MqttBridgeActorBuilder::new(&tedge_config, &smartrest_topics).await;
        let mut jwt_actor = C8YJwtRetriever::builder(
            mqtt_config.clone(),
            tedge_config.c8y.bridge.topic_prefix.clone(),
        );
        let mut http_actor = HttpActor::new().builder();
        let c8y_http_config = (&tedge_config).try_into()?;
        let mut c8y_http_proxy_actor =
            C8YHttpProxyBuilder::new(c8y_http_config, &mut http_actor, &mut jwt_actor);
        let c8y_auth_proxy_actor =
            C8yAuthProxyBuilder::try_from_config(&tedge_config, &mut jwt_actor)?;

        let mut fs_watch_actor = FsWatchActorBuilder::new();
        let mut timer_actor = TimerActor::builder();

        let identity = tedge_config.http.client.auth.identity()?;
        let mut uploader_actor = UploaderActor::new(identity.clone()).builder();
        let mut downloader_actor = DownloaderActor::new(identity).builder();

        // MQTT client dedicated to monitor the c8y-bridge client status and also
        // set service down status on shutdown, using a last-will message.
        // A separate MQTT actor/client is required as the last will message of the main MQTT actor
        // is used to send down status to health topic.
        let mut service_monitor_actor =
            MqttActorBuilder::new(service_monitor_client_config(&tedge_config)?);

        let c8y_mapper_config = C8yMapperConfig::from_tedge_config(cfg_dir, &tedge_config)?;
        let c8y_mapper_actor = C8yMapperBuilder::try_new(
            c8y_mapper_config,
            &mut mqtt_actor,
            &mut c8y_http_proxy_actor,
            &mut timer_actor,
            &mut uploader_actor,
            &mut downloader_actor,
            &mut fs_watch_actor,
            &mut service_monitor_actor,
        )?;

        // Adaptor translating commands sent on te/device/main///cmd/+/+ into requests on tedge/commands/req/+/+
        // and translating the responses received on tedge/commands/res/+/+ to te/device/main///cmd/+/+
        let old_to_new_agent_adapter = OldAgentAdapter::builder(&mut mqtt_actor);

        runtime.spawn(mqtt_actor).await?;
        runtime.spawn(jwt_actor).await?;
        runtime.spawn(http_actor).await?;
        runtime.spawn(c8y_http_proxy_actor).await?;
        runtime.spawn(c8y_auth_proxy_actor).await?;
        runtime.spawn(fs_watch_actor).await?;
        runtime.spawn(timer_actor).await?;
        runtime.spawn(c8y_mapper_actor).await?;
        runtime.spawn(service_monitor_actor).await?;
        runtime.spawn(uploader_actor).await?;
        runtime.spawn(downloader_actor).await?;
        runtime.spawn(old_to_new_agent_adapter).await?;
        runtime.spawn(bridge_actor).await?;
        runtime.run_to_completion().await?;

        Ok(())
    }
}

pub fn service_monitor_client_config(tedge_config: &TEdgeConfig) -> Result<Config, anyhow::Error> {
    let main_device_xid: EntityExternalId = tedge_config.device.id.try_read(tedge_config)?.into();
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
        .default_service_for_device(CUMULOCITY_MAPPER_NAME)
        .context("Can't derive service name if device topic id not in default scheme")?;

    let mapper_service_external_id =
        CumulocityConverter::map_to_c8y_external_id(&mapper_service_topic_id, &main_device_xid);

    let last_will_message = c8y_api::smartrest::inventory::service_creation_message(
        mapper_service_external_id.as_ref(),
        CUMULOCITY_MAPPER_NAME,
        service_type.as_str(),
        "down",
        &[],
        &tedge_config.c8y.bridge.topic_prefix,
    )?;

    let mqtt_config = tedge_config
        .mqtt_config()?
        .with_session_name("last_will_c8y_mapper")
        .with_last_will_message(last_will_message);
    Ok(mqtt_config)
}
