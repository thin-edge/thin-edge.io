use netdata_collector::MetricPoints;
use netdata_collector::TEdgeNetDataCollector;
use tedge_actors::MessageSink;
use tedge_actors::Runtime;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::ChannelFilter;
use tedge_api::mqtt_topics::EntityFilter;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_config::TEdgeConfig;
use tedge_mqtt_ext::MqttActorBuilder;
use tedge_mqtt_ext::MqttMessage;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = TEdgeConfig::load("/etc/tedge/").await?;
    let mqtt_config = config.mqtt_config()?;
    let mqtt_schema = MqttSchema::with_root(config.mqtt.topic_root.clone());

    let mut runtime = Runtime::new();
    let mut mqtt = MqttActorBuilder::new(mqtt_config.with_session_name("tedge-netdata-collect"));
    let netdata = TEdgeNetDataCollector::builder();

    let measurements = mqtt_schema.topics(EntityFilter::AnyEntity, ChannelFilter::Measurement);
    netdata.connect_mapped_source(measurements, &mut mqtt, move |msg| {
        extract_metrics(&mqtt_schema, msg)
    });

    runtime.spawn(mqtt).await?;
    runtime.spawn(netdata).await?;
    runtime.run_to_completion().await?;

    Ok(())
}

fn extract_metrics(
    schema: &MqttSchema,
    message: MqttMessage,
) -> impl Iterator<Item = MetricPoints> {
    let Ok((entity, Channel::Measurement { measurement_type })) =
        schema.entity_channel_of(&message.topic)
    else {
        return None.into_iter();
    };

    let Ok(thin_edge_json) = message.payload_str() else {
        return None.into_iter();
    };

    let device = entity
        .default_device_name()
        .unwrap_or_else(|| entity.as_str());
    let Ok(points) = MetricPoints::parse(device, &measurement_type, thin_edge_json) else {
        return None.into_iter();
    };

    Some(points).into_iter()
}
