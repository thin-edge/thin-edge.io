use crate::core::component::TEdgeComponent;
use crate::core::mapper::start_basic_actors;
use async_trait::async_trait;
use az_mapper_ext::converter::AzureConverter;
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

const AZURE_MAPPER_NAME: &str = "tedge-mapper-az";

pub struct AzureMapper;

#[async_trait]
impl TEdgeComponent for AzureMapper {
    fn session_name(&self) -> &str {
        AZURE_MAPPER_NAME
    }

    async fn start(
        &self,
        tedge_config: TEdgeConfig,
        _config_dir: &tedge_config::Path,
    ) -> Result<(), anyhow::Error> {
        let (mut runtime, mut mqtt_actor) =
            start_basic_actors(self.session_name(), &tedge_config).await?;
        if tedge_config.mqtt.bridge.built_in {
            let remote_clientid = tedge_config.device.id.try_read(&tedge_config)?;
            let rules = built_in_bridge_rules(remote_clientid)?;

            let bridge_actor = MqttBridgeActorBuilder::new(
                &tedge_config,
                "tedge-mapper-bridge-az".to_owned(),
                rules,
                &tedge_config.az.root_cert_path,
            )
            .await;
            runtime.spawn(bridge_actor).await?;
        }
        let mqtt_schema = MqttSchema::with_root(tedge_config.mqtt.topic_root.clone());
        let az_converter = AzureConverter::new(
            tedge_config.az.mapper.timestamp,
            Box::new(WallClock),
            mqtt_schema,
            tedge_config.az.mapper.timestamp_format,
        );
        let mut az_converting_actor = ConvertingActor::builder("AzConverter", az_converter);
        az_converting_actor.connect_source(get_topic_filter(&tedge_config), &mut mqtt_actor);
        az_converting_actor.connect_sink(NoConfig, &mqtt_actor);

        runtime.spawn(az_converting_actor).await?;
        runtime.spawn(mqtt_actor).await?;
        runtime.run_to_completion().await?;
        Ok(())
    }
}

fn get_topic_filter(tedge_config: &TEdgeConfig) -> TopicFilter {
    let mut topics = TopicFilter::empty();
    for topic in tedge_config.az.topics.0.clone() {
        if topics.add(&topic).is_err() {
            warn!("The configured topic '{topic}' is invalid and ignored.");
        }
    }
    topics
}

fn built_in_bridge_rules(remote_clientid: &str) -> anyhow::Result<BridgeConfig> {
    let local_prefix = "az/";
    let iothub_prefix = "$iothub/";
    let device_id_prefix = format!("devices/{remote_clientid}/");
    let mut bridge = BridgeConfig::new();
    bridge.forward_from_local("messages/events/#", local_prefix, device_id_prefix.clone())?;
    bridge.forward_from_remote("messages/devicebound/#", local_prefix, device_id_prefix)?;
    // Direct methods (request/response)
    bridge.forward_from_local("methods/res/#", local_prefix, iothub_prefix)?;
    bridge.forward_from_remote("methods/POST/#", local_prefix, iothub_prefix)?;

    // Digital twin
    bridge.forward_from_local("twin/GET/#", local_prefix, iothub_prefix)?;
    bridge.forward_from_local("twin/PATCH/#", local_prefix, iothub_prefix)?;
    bridge.forward_from_remote("twin/res/#", local_prefix, iothub_prefix)?;

    Ok(bridge)
}

#[test]
fn bridge_rules_are_valid() {
    built_in_bridge_rules("test-device-id").unwrap();
}
