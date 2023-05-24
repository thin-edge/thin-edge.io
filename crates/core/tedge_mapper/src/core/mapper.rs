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
    // Instantiate the health monitor actor, then the runtime
    let mut health_actor = HealthMonitorBuilder::new(mapper_name);
    let mut runtime = Runtime::try_new(&mut health_actor).await?;

    let mut mqtt_actor = get_mqtt_actor(mapper_name, config).await?;

    // Connect the health monitor actor to MQTT
    health_actor.connect_to_mqtt(&mut mqtt_actor);

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
