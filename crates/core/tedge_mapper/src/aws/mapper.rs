use crate::core::component::TEdgeComponent;
use crate::core::mapper::start_basic_actors;
use anyhow::Context;
use async_trait::async_trait;
use aws_mapper_ext::converter::AwsConverter;
use clock::WallClock;
use mqtt_channel::TopicFilter;
use std::str::FromStr;
use std::sync::Arc;
use tedge_actors::ConvertingActor;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::service_health_topic;
use tedge_config::all_or_nothing;
use tedge_config::models::proxy_scheme::ProxyScheme;
use tedge_config::models::TopicPrefix;
use tedge_config::tedge_toml::ProfileName;
use tedge_config::tedge_toml::TEdgeConfigReaderAws;
use tedge_config::TEdgeConfig;
use tedge_mqtt_bridge::rumqttc::Proxy;
use tedge_mqtt_bridge::rumqttc::ProxyAuth;
use tedge_mqtt_bridge::rumqttc::ProxyType;
use tedge_mqtt_bridge::rumqttc::TlsConfiguration;
use tedge_mqtt_bridge::rumqttc::Transport;
use tedge_mqtt_bridge::BridgeConfig;
use tedge_mqtt_bridge::MqttBridgeActorBuilder;
use tracing::warn;

pub struct AwsMapper {
    pub profile: Option<ProfileName>,
}

#[async_trait]
impl TEdgeComponent for AwsMapper {
    async fn start(
        &self,
        tedge_config: TEdgeConfig,
        _config_dir: &tedge_config::Path,
    ) -> Result<(), anyhow::Error> {
        let aws_config = tedge_config.aws.try_get(self.profile.as_deref())?;
        let prefix = &aws_config.bridge.topic_prefix;
        let aws_mapper_name = format!("tedge-mapper-{prefix}");
        let (mut runtime, mut mqtt_actor) =
            start_basic_actors(&aws_mapper_name, &tedge_config).await?;

        let mqtt_schema = MqttSchema::with_root(tedge_config.mqtt.topic_root.clone());
        if tedge_config.mqtt.bridge.built_in {
            let device_id = aws_config.device.id()?;
            let device_topic_id = EntityTopicId::from_str(&tedge_config.mqtt.device_topic_id)?;

            let rules = built_in_bridge_rules(device_id, prefix)?;

            let mut cloud_config = tedge_mqtt_bridge::MqttOptions::new(
                device_id,
                aws_config.url.or_config_not_set()?.to_string(),
                8883,
            );
            cloud_config.set_clean_session(false);
            cloud_config.set_keep_alive(aws_config.bridge.keepalive_interval.duration());

            let tls_config = tedge_config
                .mqtt_client_config_rustls(aws_config)
                .context("Failed to create MQTT TLS config")?;
            cloud_config.set_transport(Transport::tls_with_config(tls_config.into()));

            let rustls_config = tedge_config.cloud_client_tls_config();
            let proxy_config = &tedge_config.proxy;
            if let Some(address) = proxy_config.address.or_none() {
                let credentials =
                    all_or_nothing((proxy_config.username.clone(), proxy_config.password.clone()))
                        .map_err(|e| anyhow::anyhow!(e))?;
                cloud_config.set_proxy(Proxy {
                    addr: address.host().to_string(),
                    port: address.port().0,
                    auth: match credentials {
                        Some((username, password)) => ProxyAuth::Basic { username, password },
                        None => ProxyAuth::None,
                    },
                    ty: match address.scheme() {
                        ProxyScheme::Http => ProxyType::Http,
                        ProxyScheme::Https => {
                            ProxyType::Https(TlsConfiguration::Rustls(Arc::new(rustls_config)))
                        }
                    },
                });
            }

            let bridge_name = format!("tedge-mapper-bridge-{prefix}");
            let health_topic = service_health_topic(&mqtt_schema, &device_topic_id, &bridge_name);

            let bridge_actor = MqttBridgeActorBuilder::new(
                &tedge_config,
                &bridge_name,
                &health_topic,
                rules,
                cloud_config,
            )
            .await;
            runtime.spawn(bridge_actor).await?;
        }
        let clock = Box::new(WallClock);
        let aws_converter = AwsConverter::new(
            aws_config.mapper.timestamp,
            clock,
            mqtt_schema,
            aws_config.mapper.timestamp_format,
            prefix.clone(),
            aws_config.mapper.mqtt.max_payload_size.0,
        );
        let mut aws_converting_actor = ConvertingActor::builder("AwsConverter", aws_converter);

        aws_converting_actor.connect_source(get_topic_filter(aws_config), &mut mqtt_actor);
        aws_converting_actor.connect_sink(NoConfig, &mqtt_actor);

        runtime.spawn(aws_converting_actor).await?;
        runtime.spawn(mqtt_actor).await?;
        runtime.run_to_completion().await?;
        Ok(())
    }
}

fn get_topic_filter(aws_config: &TEdgeConfigReaderAws) -> TopicFilter {
    let mut topics = TopicFilter::empty();
    for topic in aws_config.topics.0.clone() {
        if topics.add(&topic).is_err() {
            warn!("The configured topic '{topic}' is invalid and ignored.");
        }
    }
    topics
}

fn built_in_bridge_rules(
    remote_client_id: &str,
    topic_prefix: &TopicPrefix,
) -> Result<BridgeConfig, anyhow::Error> {
    let local_prefix = format!("{topic_prefix}/");
    let device_id_prefix = format!("thinedge/{remote_client_id}/");
    let things_prefix = format!("$aws/things/{remote_client_id}/");
    let conn_check = format!("thinedge/devices/{remote_client_id}/test-connection");
    let mut bridge = BridgeConfig::new();

    // telemetry/command topics for use by the user
    bridge.forward_from_local("td/#", local_prefix.clone(), device_id_prefix.clone())?;
    bridge.forward_from_remote("cmd/#", local_prefix.clone(), device_id_prefix)?;

    // topic to interact with the shadow of the device
    bridge.forward_bidirectionally("shadow/#", local_prefix.clone(), things_prefix.clone())?;

    // echo topic mapping to check the connection
    bridge.forward_from_local(
        "",
        format!("{local_prefix}test-connection"),
        conn_check.clone(),
    )?;
    bridge.forward_from_remote("", format!("{local_prefix}connection-success"), conn_check)?;

    Ok(bridge)
}

#[test]
fn bridge_rules_are_valid() {
    built_in_bridge_rules("test-device-id", &"aws".try_into().unwrap()).unwrap();
}
