use crate::core::component::TEdgeComponent;
use crate::core::mapper::start_basic_actors;
use async_trait::async_trait;
use c8y_http_proxy::credentials::C8YJwtRetriever;
use c8y_http_proxy::C8YHttpProxyBuilder;
use c8y_mapper_ext::actor::C8yMapperBuilder;
use c8y_mapper_ext::config::C8yMapperConfig;
use c8y_mapper_ext::service_monitor::service_monitor_status_message;
use mqtt_channel::Config;
use std::path::Path;
use tedge_config::new::TEdgeConfig;
use tedge_file_system_ext::FsWatchActorBuilder;
use tedge_http_ext::HttpActor;
use tedge_mqtt_ext::MqttActorBuilder;
use tedge_timer_ext::TimerActor;

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
        let mut jwt_actor = C8YJwtRetriever::builder(mqtt_config.clone());
        let mut http_actor = HttpActor::new().builder();
        let c8y_http_config = (&tedge_config).try_into()?;
        let mut c8y_http_proxy_actor =
            C8YHttpProxyBuilder::new(c8y_http_config, &mut http_actor, &mut jwt_actor);

        let mut fs_watch_actor = FsWatchActorBuilder::new();
        let mut timer_actor = TimerActor::builder();

        let c8y_mapper_config = C8yMapperConfig::from_tedge_config(cfg_dir, &tedge_config)?;
        let c8y_mapper_actor = C8yMapperBuilder::try_new(
            c8y_mapper_config,
            &mut mqtt_actor,
            &mut c8y_http_proxy_actor,
            &mut timer_actor,
            &mut fs_watch_actor,
        )?;

        // MQTT client dedicated to set service down status on shutdown, using a last-will message
        // A separate MQTT actor/client is required as the last will message of the main MQTT actor
        // is used to send down status to tedge/health topic
        let service_monitor_actor =
            MqttActorBuilder::new(service_monitor_client_config(&tedge_config)?);

        runtime.spawn(mqtt_actor).await?;
        runtime.spawn(jwt_actor).await?;
        runtime.spawn(http_actor).await?;
        runtime.spawn(c8y_http_proxy_actor).await?;
        runtime.spawn(fs_watch_actor).await?;
        runtime.spawn(timer_actor).await?;
        runtime.spawn(c8y_mapper_actor).await?;
        runtime.spawn(service_monitor_actor).await?;
        runtime.run_to_completion().await?;

        Ok(())
    }
}

pub fn service_monitor_client_config(tedge_config: &TEdgeConfig) -> Result<Config, anyhow::Error> {
    let device_name = tedge_config.device.id.try_read(tedge_config)?.to_string();
    let service_type = tedge_config.service.ty.clone();

    let mqtt_config = tedge_config
        .mqtt_config()?
        .with_session_name("last_will_c8y_mapper")
        .with_last_will_message(service_monitor_status_message(
            &device_name,
            CUMULOCITY_MAPPER_NAME,
            "down",
            &service_type,
            None,
        ));
    Ok(mqtt_config)
}
