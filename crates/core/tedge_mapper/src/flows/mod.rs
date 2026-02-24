use crate::core::mapper::start_basic_actors;
use crate::core::mqtt::flows_status_topic;
use crate::TEdgeComponent;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_config::TEdgeConfig;
use tedge_file_system_ext::FsWatchActorBuilder;
use tedge_flows::ConnectedFlowRegistry;
use tedge_flows::FlowsMapperBuilder;
use tedge_watch_ext::WatchActorBuilder;

pub struct GenMapper;

#[async_trait::async_trait]
impl TEdgeComponent for GenMapper {
    async fn start(
        &self,
        tedge_config: TEdgeConfig,
        config_dir: &tedge_config::Path,
    ) -> Result<(), anyhow::Error> {
        let mapper_name = "tedge-mapper-local";
        let (mut runtime, mut mqtt_actor) = start_basic_actors(mapper_name, &tedge_config).await?;

        let mqtt_schema = MqttSchema::with_root(tedge_config.mqtt.topic_root.clone());
        let service_topic_id = EntityTopicId::default_main_service(mapper_name)?;

        let mut fs_actor = FsWatchActorBuilder::new();
        let mut cmd_watcher_actor = WatchActorBuilder::new();
        let flows_dir = tedge_flows::default_flows_dir(config_dir);
        let flows = ConnectedFlowRegistry::new(flows_dir);
        let flows_status = flows_status_topic(&mqtt_schema, &service_topic_id);
        let mut flows_mapper = FlowsMapperBuilder::try_new(flows, flows_status).await?;
        flows_mapper.connect(&mut mqtt_actor);
        flows_mapper.connect_fs(&mut fs_actor);
        flows_mapper.connect_cmd(&mut cmd_watcher_actor);

        runtime.spawn(flows_mapper).await?;
        runtime.spawn(mqtt_actor).await?;
        runtime.spawn(fs_actor).await?;
        runtime.spawn(cmd_watcher_actor).await?;
        runtime.run_to_completion().await?;
        Ok(())
    }
}
