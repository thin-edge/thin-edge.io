use miette::IntoDiagnostic;
#[cfg(test)]
use std::result::Result::Ok;
use tedge_actors::Runtime;
use tedge_api::mqtt_topics::DeviceTopicId;
use tedge_api::mqtt_topics::EntityTopicId;
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
) -> Result<(Runtime, MqttActorBuilder), miette::Error> {
    let runtime_events_logger = None;
    let mut runtime = Runtime::try_new(runtime_events_logger).await?;

    let mut mqtt_actor = get_mqtt_actor(mapper_name, config).await?;

    //Instantiate health monitor actor
    let service = Service {
        service_topic_id: ServiceTopicId::new(
            EntityTopicId::default_main_service(mapper_name).unwrap(),
        ),
        device_topic_id: DeviceTopicId::new(EntityTopicId::default_main_device()),
    };
    let mqtt_schema = MqttSchema::with_root(config.mqtt.topic_root.clone());
    let health_actor = HealthMonitorBuilder::from_service_topic_id(
        service,
        &mut mqtt_actor,
        &mqtt_schema,
        config.service.ty.clone(),
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
) -> Result<MqttActorBuilder, miette::Error> {
    let mqtt_config = tedge_config.mqtt_config().into_diagnostic()?;

    Ok(MqttActorBuilder::new(
        mqtt_config.with_session_name(session_name),
    ))
}
