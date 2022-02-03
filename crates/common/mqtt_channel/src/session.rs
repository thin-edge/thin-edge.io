use crate::{Config, Connection, MqttError};
use rumqttc::{AsyncClient, Event, Packet};

/// Create a session using the `config.session_name`
///
/// The config can be used to connect later using `Connection::new(config)`.
/// All the messages that have been published meantime with `QoS > 1`
/// on the `config.subscriptions` topics will be received by the new connection.
///
/// `mqtt_channel::init_session(&config) consumes no messages.
pub async fn init_session(config: &Config) -> Result<(), MqttError> {
    if config.clean_session || config.session_name.is_none() {
        return Err(MqttError::InvalidSessionConfig);
    }

    let mqtt_options = config.mqtt_options();
    let (mqtt_client, mut event_loop) = AsyncClient::new(mqtt_options, config.queue_capacity);

    loop {
        match event_loop.poll().await {
            Ok(Event::Incoming(Packet::ConnAck(_))) => {
                let subscriptions = config.subscriptions.filters();
                if subscriptions.is_empty() {
                    break;
                }
                mqtt_client.subscribe_many(subscriptions).await?;
            }

            Ok(Event::Incoming(Packet::SubAck(_))) => {
                break;
            }

            Err(err) => {
                let delay = Connection::pause_on_error(&err);

                if delay {
                    Connection::do_pause().await;
                }
            }
            _ => (),
        }
    }

    Ok(())
}

/// Clear any session named as `config.session_name`
///
/// The config can be used to connect later using `Connection::new(config)`.
/// No messages that have been published meantime will be received by the new connection.
pub async fn clear_session(config: &Config) -> Result<(), MqttError> {
    if config.session_name.is_none() {
        return Err(MqttError::InvalidSessionConfig);
    }
    let mut mqtt_options = config.mqtt_options();
    mqtt_options.set_clean_session(true);
    let (mqtt_client, mut event_loop) = AsyncClient::new(mqtt_options, config.queue_capacity);

    loop {
        match event_loop.poll().await {
            Ok(Event::Incoming(Packet::ConnAck(_))) => {
                let subscriptions = config.subscriptions.filters();
                if subscriptions.is_empty() {
                    break;
                }
                for s in subscriptions.iter() {
                    mqtt_client.unsubscribe(&s.path).await?;
                }
            }

            Ok(Event::Incoming(Packet::UnsubAck(_))) => {
                break;
            }

            Err(err) => {
                let delay = Connection::pause_on_error(&err);

                if delay {
                    Connection::do_pause().await;
                }
            }
            _ => (),
        }
    }

    Ok(())
}
