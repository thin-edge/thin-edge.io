use crate::core::component::TEdgeComponent;
use crate::core::mapper::start_basic_actors;
use async_trait::async_trait;
use aws_mapper_ext::converter::AwsConverter;
use clock::WallClock;
use mqtt_channel::TopicFilter;
use tedge_actors::ConvertingActor;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_config::TEdgeConfig;
use tedge_mqtt_bridge::BridgeConfig;
use tedge_mqtt_bridge::MqttBridgeActorBuilder;
use tracing::warn;

const AWS_MAPPER_NAME: &str = "tedge-mapper-aws";

pub struct AwsMapper;

#[async_trait]
impl TEdgeComponent for AwsMapper {
    fn session_name(&self) -> &str {
        AWS_MAPPER_NAME
    }

    async fn start(
        &self,
        tedge_config: TEdgeConfig,
        _config_dir: &tedge_config::Path,
    ) -> Result<(), anyhow::Error> {
        let (mut runtime, mut mqtt_actor) =
            start_basic_actors(self.session_name(), &tedge_config).await?;
        if tedge_config.mqtt.bridge.built_in {
            let device_id = tedge_config.device.id.try_read(&tedge_config)?;
            let rules = built_in_bridge_rules(device_id)?;
            let bridge_actor = MqttBridgeActorBuilder::new(
                &tedge_config,
                "tedge-mapper-bridge-az".to_owned(),
                rules,
                &tedge_config.az.root_cert_path,
            )
            .await;
            runtime.spawn(bridge_actor).await?;
        }
        let clock = Box::new(WallClock);
        let mqtt_schema = MqttSchema::with_root(tedge_config.mqtt.topic_root.clone());
        let aws_converter = AwsConverter::new(
            tedge_config.aws.mapper.timestamp,
            clock,
            mqtt_schema,
            tedge_config.aws.mapper.timestamp_format,
        );
        let mut aws_converting_actor = ConvertingActor::builder("AwsConverter", aws_converter);

        aws_converting_actor.connect_source(get_topic_filter(&tedge_config), &mut mqtt_actor);
        aws_converting_actor.connect_sink(NoConfig, &mqtt_actor);

        runtime.spawn(aws_converting_actor).await?;
        runtime.spawn(mqtt_actor).await?;
        runtime.run_to_completion().await?;
        Ok(())
    }
}

fn get_topic_filter(tedge_config: &TEdgeConfig) -> TopicFilter {
    let mut topics = TopicFilter::empty();
    for topic in tedge_config.aws.topics.0.clone() {
        if topics.add(&topic).is_err() {
            warn!("The configured topic '{topic}' is invalid and ignored.");
        }
    }
    topics
}

fn built_in_bridge_rules(remote_client_id: &str) -> Result<BridgeConfig, anyhow::Error> {
    let local_prefix = "aws/";
    let device_id_prefix = format!("thinedge/{remote_client_id}/");
    let things_prefix = format!("$aws/things/{remote_client_id}/");
    let conn_check = format!("thinedge/devices/{remote_client_id}/test-connection");
    let mut bridge = BridgeConfig::new();

    // telemetry/command topics for use by the user
    bridge.forward_from_local("td/#", local_prefix, device_id_prefix.clone())?;
    bridge.forward_from_remote("cmd/#", local_prefix, device_id_prefix)?;

    // topic to interact with the shadow of the device
    bridge.forward_from_local("shadow/#", local_prefix, things_prefix.clone())?;
    bridge.forward_from_remote("shadow/#", local_prefix, things_prefix)?;

    // echo topic mapping to check the connection
    bridge.forward_from_local("", "aws/test-connection", conn_check.clone())?;
    bridge.forward_from_remote("", "aws/connection-success", conn_check)?;

    Ok(bridge)
}

#[test]
fn bridge_rules_are_valid() {
    built_in_bridge_rules("test-device-id").unwrap();
}
