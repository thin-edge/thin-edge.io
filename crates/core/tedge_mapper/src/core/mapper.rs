#[cfg(test)]
use std::result::Result::Ok;
use tedge_actors::Runtime;
use tedge_config::new::TEdgeConfig;
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

    runtime.spawn(signal_actor).await?;
    runtime.spawn(health_actor).await?;
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
