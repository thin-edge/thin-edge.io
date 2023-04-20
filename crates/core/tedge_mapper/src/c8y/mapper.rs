use super::actor::C8yMapperBuilder;
use super::config::C8yMapperConfig;
use super::service_monitor::service_monitor_status_message;
use crate::core::component::TEdgeComponent;
use crate::core::mapper::start_basic_actors;
use async_trait::async_trait;
use c8y_api::smartrest::operations::Operations;
use c8y_api::smartrest::topic::C8yTopic;
use c8y_http_proxy::credentials::C8YJwtRetriever;
use c8y_http_proxy::C8YHttpProxyBuilder;
use mqtt_channel::Connection;
use mqtt_channel::TopicFilter;
use std::path::Path;
use tedge_actors::MessageSource;
use tedge_actors::ServiceConsumer;
use tedge_api::topic::ResponseTopic;
use tedge_config::TEdgeConfig;
use tedge_file_system_ext::FsWatchActorBuilder;
use tedge_http_ext::HttpActor;
use tedge_timer_ext::TimerActor;
use tedge_utils::file::*;
use tracing::info;

const CUMULOCITY_MAPPER_NAME: &str = "tedge-mapper-c8y";

pub struct CumulocityMapper {}

impl CumulocityMapper {
    pub fn new() -> CumulocityMapper {
        CumulocityMapper {}
    }

    pub fn subscriptions(operations: &Operations) -> Result<TopicFilter, anyhow::Error> {
        let mut topic_filter: TopicFilter = vec![
            "tedge/measurements",
            "tedge/measurements/+",
            "tedge/alarms/+/+",
            "tedge/alarms/+/+/+",
            "c8y-internal/alarms/+/+",
            "c8y-internal/alarms/+/+/+",
            "tedge/events/+",
            "tedge/events/+/+",
            "tedge/health/+",
            "tedge/health/+/+",
            C8yTopic::SmartRestRequest.to_string().as_str(),
            ResponseTopic::SoftwareListResponse.as_str(),
            ResponseTopic::SoftwareUpdateResponse.as_str(),
            ResponseTopic::RestartResponse.as_str(),
        ]
        .try_into()
        .expect("topics that mapper should subscribe to");

        for topic in operations.topics_for_operations() {
            topic_filter.add(&topic)?
        }

        Ok(topic_filter)
    }
}

#[async_trait]
impl TEdgeComponent for CumulocityMapper {
    fn session_name(&self) -> &str {
        CUMULOCITY_MAPPER_NAME
    }

    async fn init(&self, cfg_dir: &Path) -> Result<(), anyhow::Error> {
        info!("Initialize tedge mapper c8y");
        create_directories(cfg_dir)?;

        // HIPPO: Are these subscriptions still needed on init?
        let operations = Operations::try_new(format!("{}/operations/c8y", cfg_dir.display()))?;
        self.init_session(CumulocityMapper::subscriptions(&operations)?)
            .await?;
        Ok(())
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
        let mut c8y_mapper_actor = C8yMapperBuilder::new(c8y_mapper_config);

        // Connect other actor instances to config manager actor
        c8y_mapper_actor.set_connection(&mut mqtt_actor);
        c8y_mapper_actor.set_connection(&mut timer_actor);
        fs_watch_actor.add_sink(&c8y_mapper_actor);
        c8y_mapper_actor.with_c8y_http_proxy(&mut c8y_http_proxy_actor)?;

        runtime.spawn(mqtt_actor).await?;
        runtime.spawn(jwt_actor).await?;
        runtime.spawn(http_actor).await?;
        runtime.spawn(c8y_http_proxy_actor).await?;
        runtime.spawn(fs_watch_actor).await?;
        runtime.spawn(timer_actor).await?;
        runtime.spawn(c8y_mapper_actor).await?;
        runtime.run_to_completion().await?;

        Ok(())
    }
}

pub async fn create_mqtt_client_will_message(
    device_name: &str,
    app_name: &str,
    service_type: &str,
    tedge_config: &TEdgeConfig,
) -> Result<Connection, anyhow::Error> {
    let mqtt_config = tedge_config
        .mqtt_config()?
        .with_session_name("last_will_c8y_mapper")
        .with_last_will_message(service_monitor_status_message(
            device_name,
            app_name,
            "down",
            service_type,
            None,
        ));
    let mqtt_client = Connection::new(&mqtt_config).await?;

    Ok(mqtt_client)
}

fn create_directories(config_dir: &Path) -> Result<(), anyhow::Error> {
    create_directory_with_user_group(
        format!("{}/operations/c8y", config_dir.display()),
        "tedge",
        "tedge",
        0o775,
    )?;
    create_file_with_user_group(
        format!("{}/operations/c8y/c8y_SoftwareUpdate", config_dir.display()),
        "tedge",
        "tedge",
        0o644,
        None,
    )?;
    create_file_with_user_group(
        format!("{}/operations/c8y/c8y_Restart", config_dir.display()),
        "tedge",
        "tedge",
        0o644,
        None,
    )?;
    // Create directory for device custom fragments
    create_directory_with_user_group(
        format!("{}/device", config_dir.display()),
        "tedge",
        "tedge",
        0o775,
    )?;
    Ok(())
}
