use crate::tedge_to_te_converter::converter::TedgetoTeConverter;
use mqtt_channel::TopicFilter;
#[cfg(test)]
use std::result::Result::Ok;
use tedge_actors::{ConvertingActor, ConvertingActorBuilder, MessageSink, MessageSource, Runtime};
use tedge_config::TEdgeConfig;
use tedge_health_ext::HealthMonitorBuilder;
use tedge_mqtt_ext::MqttActorBuilder;
use tedge_signal_ext::SignalActor;

pub async fn start_basic_actors(
    mapper_name: &str,
    config: &TEdgeConfig,
) -> Result<(Runtime, MqttActorBuilder), anyhow::Error> {
    let runtime_events_logger = None;
    let mut runtime = Runtime::try_new(runtime_events_logger).await?;

    let mut mqtt_actor = get_mqtt_actor(mapper_name, config).await?;

    //Instantiate health monitor actor
    let health_actor = HealthMonitorBuilder::new(mapper_name, &mut mqtt_actor);

    // Shutdown on SIGINT
    let signal_actor = SignalActor::builder(&runtime.get_handle());

    let converter_actor = create_tedge_to_te_converter(&mut mqtt_actor)?;

    runtime.spawn(signal_actor).await?;
    runtime.spawn(health_actor).await?;
    runtime.spawn(converter_actor).await?;

    Ok((runtime, mqtt_actor))
}

async fn get_mqtt_actor(
    session_name: &str,
    tedge_config: &TEdgeConfig,
) -> Result<MqttActorBuilder, anyhow::Error> {
    let mqtt_config = tedge_config.mqtt_config()?;

    Ok(MqttActorBuilder::new(
        mqtt_config.with_session_name(session_name),
    ))
}

pub fn create_tedge_to_te_converter(
    mqtt_actor_builder: &mut MqttActorBuilder,
) -> Result<ConvertingActorBuilder<TedgetoTeConverter, TopicFilter>, anyhow::Error> {
    let tedge_to_te_converter = TedgetoTeConverter::new();
    let subscriptions: TopicFilter = vec![
        "tedge/measurements",
        "tedge/measurements/+",
        "tedge/events/+",
        "tedge/events/+/+",
        "tedge/alarms/+/+",
        "tedge/alarms/+/+/+",
        "tedge/health/+",
        "tedge/health/+/+",
    ]
    .try_into()?;

    // Tedge to Te converter
    let mut tedge_converter_actor =
        ConvertingActor::builder("TedgetoTeConverter", tedge_to_te_converter, subscriptions);

    tedge_converter_actor.add_input(mqtt_actor_builder);
    tedge_converter_actor.add_sink(mqtt_actor_builder);

    Ok(tedge_converter_actor)
}
