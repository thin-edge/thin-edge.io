use crate::core::component::TEdgeComponent;
use crate::core::mapper::start_basic_actors;
use async_trait::async_trait;
use az_mapper_ext::converter::AzureConverter;
use clock::WallClock;
use mqtt_channel::TopicFilter;
use std::borrow::Cow;
use std::str::FromStr;
use tedge_actors::ConvertingActor;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::service_health_topic;
use tedge_config::TEdgeConfig;
use tedge_config::TEdgeConfigReaderAz;
use tedge_config::TopicPrefix;
use tedge_mqtt_bridge::use_key_and_cert;
use tedge_mqtt_bridge::BridgeConfig;
use tedge_mqtt_bridge::MqttBridgeActorBuilder;
use tracing::warn;

pub struct AzureMapper {
    pub profile: Option<String>,
}

#[async_trait]
impl TEdgeComponent for AzureMapper {
    async fn start(
        &self,
        tedge_config: TEdgeConfig,
        _config_dir: &tedge_config::Path,
    ) -> Result<(), anyhow::Error> {
        let az_config = tedge_config.az.try_get(self.profile.as_deref())?;
        let prefix = &az_config.bridge.topic_prefix;
        let az_mapper_name = format!("tedge-mapper-{prefix}");
        let (mut runtime, mut mqtt_actor) =
            start_basic_actors(&az_mapper_name, &tedge_config).await?;
        let mqtt_schema = MqttSchema::with_root(tedge_config.mqtt.topic_root.clone());

        if tedge_config.mqtt.bridge.built_in {
            let device_topic_id = EntityTopicId::from_str(&tedge_config.mqtt.device_topic_id)?;

            let remote_clientid = tedge_config.device.id.try_read(&tedge_config)?;
            let rules = built_in_bridge_rules(remote_clientid, prefix)?;

            let mut cloud_config = tedge_mqtt_bridge::MqttOptions::new(
                remote_clientid,
                az_config.url.or_config_not_set()?.to_string(),
                8883,
            );
            cloud_config.set_clean_session(false);
            cloud_config.set_credentials(
                format!(
                    "{}/{remote_clientid}/?api-version=2018-06-30",
                    az_config.url.or_config_not_set()?
                ),
                "",
            );
            use_key_and_cert(&mut cloud_config, &az_config.root_cert_path, &tedge_config)?;

            let built_in_bridge_name = format!("tedge-mapper-bridge-{prefix}");
            let health_topic =
                service_health_topic(&mqtt_schema, &device_topic_id, &built_in_bridge_name);

            let bridge_actor = MqttBridgeActorBuilder::new(
                &tedge_config,
                &built_in_bridge_name,
                &health_topic,
                rules,
                cloud_config,
            )
            .await;
            runtime.spawn(bridge_actor).await?;
        }
        let mqtt_schema = MqttSchema::with_root(tedge_config.mqtt.topic_root.clone());
        let az_converter = AzureConverter::new(
            az_config.mapper.timestamp,
            Box::new(WallClock),
            mqtt_schema,
            az_config.mapper.timestamp_format,
            prefix,
            az_config.mapper.mqtt.max_payload_size.0,
        );
        let mut az_converting_actor = ConvertingActor::builder("AzConverter", az_converter);
        az_converting_actor.connect_source(get_topic_filter(az_config), &mut mqtt_actor);
        az_converting_actor.connect_sink(NoConfig, &mqtt_actor);

        runtime.spawn(az_converting_actor).await?;
        runtime.spawn(mqtt_actor).await?;
        runtime.run_to_completion().await?;
        Ok(())
    }
}

fn get_topic_filter(az_config: &TEdgeConfigReaderAz) -> TopicFilter {
    let mut topics = TopicFilter::empty();
    for topic in az_config.topics.0.clone() {
        if topics.add(&topic).is_err() {
            warn!("The configured topic '{topic}' is invalid and ignored.");
        }
    }
    topics
}

fn built_in_bridge_rules(
    remote_clientid: &str,
    local_prefix: &TopicPrefix,
) -> anyhow::Result<BridgeConfig> {
    let local_prefix: Cow<str> = Cow::Owned(format!("{local_prefix}/"));
    let iothub_prefix = "$iothub/";
    let device_id_prefix = format!("devices/{remote_clientid}/");
    let mut bridge = BridgeConfig::new();
    bridge.forward_from_local(
        "messages/events/#",
        local_prefix.clone(),
        device_id_prefix.clone(),
    )?;
    bridge.forward_from_remote(
        "messages/devicebound/#",
        local_prefix.clone(),
        device_id_prefix,
    )?;
    // Direct methods (request/response)
    bridge.forward_from_local("methods/res/#", local_prefix.clone(), iothub_prefix)?;
    bridge.forward_from_remote("methods/POST/#", local_prefix.clone(), iothub_prefix)?;

    // Digital twin
    bridge.forward_from_local("twin/GET/#", local_prefix.clone(), iothub_prefix)?;
    bridge.forward_from_local("twin/PATCH/#", local_prefix.clone(), iothub_prefix)?;
    bridge.forward_from_remote("twin/res/#", local_prefix.clone(), iothub_prefix)?;

    Ok(bridge)
}

#[test]
fn bridge_rules_are_valid() {
    built_in_bridge_rules("test-device-id", &"az".try_into().unwrap()).unwrap();
}
