#[cfg(test)]
use std::result::Result::Ok;
use tedge_actors::Runtime;
use tedge_api::mqtt_topics::DeviceTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::Service;
use tedge_api::mqtt_topics::ServiceTopicId;
use tedge_config::TEdgeConfig;
use tedge_health_ext::HealthMonitorBuilder;
use tedge_mqtt_ext::MqttActorBuilder;
use tedge_signal_ext::SignalActor;

pub async fn start_basic_actors(
    mapper_name: &str,
    config: &TEdgeConfig,
) -> Result<(Runtime, MqttActorBuilder), anyhow::Error> {
    let mut runtime = Runtime::new();

    let device_topic_id = &config.mqtt.device_topic_id;
    let session_name = if device_topic_id.is_default_main_device() {
        mapper_name.to_string()
    } else {
        format!("{mapper_name}#{device_topic_id}")
    };
    let mut mqtt_actor = get_mqtt_actor(&session_name, config).await?;

    //Instantiate health monitor actor
    let service = Service {
        service_topic_id: ServiceTopicId::new(
            device_topic_id
                .default_service_for_device(mapper_name)
                .unwrap(),
        ),
        device_topic_id: DeviceTopicId::new(device_topic_id.clone()),
    };
    let mqtt_schema = MqttSchema::with_root(config.mqtt.topic_root.clone());
    let health_actor = HealthMonitorBuilder::from_service_topic_id(
        service,
        &mut mqtt_actor,
        &mqtt_schema,
        &config.service,
    );

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
