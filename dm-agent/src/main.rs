use futures::future::FutureExt;
use mqtt_client::{Client, Message, MessageStream, QoS, Topic, TopicFilter};
use std::str::from_utf8;
use std::sync::Arc;
use thin_edge_json::{
    group::MeasurementGrouper, measurement::current_timestamp, measurement::FlatMeasurementVisitor,
    serialize::ThinEdgeJsonSerializer,
};
use tokio::{select, time::Duration};
use tracing::{debug_span, error, info, instrument, Instrument};

const DEFAULT_STATS_COLLECTION_WINDOW: u64 = 1000;

const DEFAULT_HOST: &str = "localhost";
const DEFAULT_PORT: u16 = 1883;
const CLIENT_ID: &str = "tedge-dm-agent";
const SOURCE_TOPIC: &str = "collectd/#";
const TARGET_TOPIC: &str = "tedge/measurements";
const METRIC_GROUP_KEY_POS: usize = 2;
const METRIC_KEY_POS: usize = 3;

const APP_NAME: &str = "tedge-dm-agent";
const DEFAULT_LOG_LEVEL: &str = "warn";
const TIME_FORMAT: &str = "%Y-%m-%dT%H:%M:%S%.3f%:z";
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    MqttError(#[from] Arc<mqtt_client::Error>), //TODO: avoid the logical duplication of this error and MqttClientError because of the Arc

    #[error(transparent)]
    MqttClientError(#[from] mqtt_client::Error),

    #[error(
        "Message received on invalid collectd topic: {0}.
        Collectd message topics must be in the format collectd/<hostname>/<metric-plugin-name>/<metric-key>"
    )]
    InvalidMeasurementTopic(String),

    #[error("Non UTF-8 payload received on topic: {0}")]
    InvalidMeasurementPayload(String),

    #[error(
        "Invalid payload: {0} received on topic: {1}. Expected payload format: <timestamp>:<value>"
    )]
    InvalidMeasurementPayloadFormat(String, String),

    #[error("Invalid measurement value: {0} received on topic: {1}. Must be a number")]
    InvalidMeasurementValue(String, String),

    #[error("Invalid Thin Edge JSON measurement format")]
    InvalidThinEdgeJsonError(#[from] thin_edge_json::group::MeasurementGrouperError),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| DEFAULT_LOG_LEVEL.into());
    tracing_subscriber::fmt()
        .with_timer(tracing_subscriber::fmt::time::ChronoUtc::with_format(
            TIME_FORMAT.into(),
        ))
        .with_env_filter(filter)
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        .init();

    info!("{} starting!", APP_NAME);
    run().instrument(debug_span!(APP_NAME)).await?;

    Ok(())
}

#[instrument(name = "main")]
async fn run() -> Result<(), Error> {
    let config = mqtt_client::Config::new(DEFAULT_HOST, DEFAULT_PORT);
    let mqtt = Arc::new(Client::connect(CLIENT_ID, &config).await?);
    let filter = TopicFilter::new(SOURCE_TOPIC)?.qos(QoS::AtMostOnce);

    let mut errors = mqtt.subscribe_errors();
    let mut messages: MessageStream = mqtt.subscribe(filter).await?;

    let mut interval =
        tokio::time::interval(Duration::from_millis(DEFAULT_STATS_COLLECTION_WINDOW));

    let mut message_batch = MeasurementGrouper::new();

    loop {
        select! {
            error = errors.next().fuse() => {
                if let Some(err) = error {
                    error!("{}", err);
                }
            }

            maybe_message = messages.next().fuse() => {
                if let Some(message) = maybe_message {
                    if message_batch.is_empty() {
                        let _ = message_batch.timestamp(&current_timestamp()).map_err(|err| error!("{}", err));
                    }
                    let _ = tedge_json_from_collectd_message(&mut message_batch, message).map_err(|err| error!("{}", err));
                }
            }

            _result = interval.tick() => {
                if !message_batch.is_empty() {
                    handle_message_batch(mqtt.clone(), message_batch);
                    message_batch = MeasurementGrouper::new();
                }
            }
        }
    }
}

fn tedge_json_from_collectd_message(
    message_batch: &mut MeasurementGrouper,
    message: Message,
) -> Result<(), Error> {
    let topic = message.topic.name.clone();
    let split = topic.split('/').collect::<Vec<&str>>();
    if split.len() == 4 {
        let group_measurement_key = split[METRIC_GROUP_KEY_POS];
        let measurement_key = split[METRIC_KEY_POS];

        let payload = message.payload;
        let payload =
            from_utf8(&payload).map_err(|_err| Error::InvalidMeasurementPayload(topic.clone()))?; //TODO: avoid topic clone?
        let split = payload.split(':').collect::<Vec<&str>>();

        if split.len() == 2 {
            let measurement_value = split[1]
                .trim_end_matches(char::from(0))
                .parse::<f64>()
                .map_err(|_err| Error::InvalidMeasurementValue(split[1].into(), topic.clone()))?;
            message_batch.measurement(
                Some(group_measurement_key),
                measurement_key,
                measurement_value,
            )?;
        } else {
            return Err(Error::InvalidMeasurementPayloadFormat(
                payload.into(),
                topic.clone(),
            ));
        }
    } else {
        //TODO: Why explicit return
        return Err(Error::InvalidMeasurementTopic(topic));
    }

    Ok(())
}

#[instrument(name = "handle_message_batch")]
fn handle_message_batch(mqtt: Arc<Client>, message_batch: MeasurementGrouper) {
    tokio::task::spawn(async move {
        if let Err(err) = process_message_batch(mqtt, message_batch).await {
            error!("{}", err);
        }
    });
}

async fn process_message_batch(
    mqtt: Arc<Client>,
    message_batch: MeasurementGrouper,
) -> anyhow::Result<()> {
    let mut tedge_json_serializer = ThinEdgeJsonSerializer::new();
    message_batch.accept(&mut tedge_json_serializer)?;

    let topic = Topic::new(TARGET_TOPIC).unwrap();
    let tedge_message = Message::new(&topic, tedge_json_serializer.bytes()?);

    mqtt.as_ref().publish(tedge_message).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{i128::MIN, u128::MAX};

    use assert_matches::assert_matches;

    use super::*;

    #[test]
    fn test_message_batch_processor() {
        let mut message_batch = MeasurementGrouper::new();

        let topic = Topic::new("collectd/localhost/temperature/value").unwrap();
        let collectd_message = Message::new(&topic, "123456789:32.5");
        tedge_json_from_collectd_message(&mut message_batch, collectd_message).unwrap();

        let topic = Topic::new("collectd/localhost/coordinate/x").unwrap();
        let collectd_message = Message::new(&topic, "123456789:50");
        tedge_json_from_collectd_message(&mut message_batch, collectd_message).unwrap();

        let topic = Topic::new("collectd/localhost/coordinate/y").unwrap();
        let collectd_message = Message::new(&topic, "123456789:70");
        tedge_json_from_collectd_message(&mut message_batch, collectd_message).unwrap();

        let topic = Topic::new("collectd/localhost/pressure/value").unwrap();
        let collectd_message = Message::new(&topic, "123456789:98.2");
        tedge_json_from_collectd_message(&mut message_batch, collectd_message).unwrap();

        let topic = Topic::new("collectd/localhost/coordinate/z").unwrap();
        let collectd_message = Message::new(&topic, "123456789:90");
        tedge_json_from_collectd_message(&mut message_batch, collectd_message).unwrap();

        assert_eq!(
            message_batch.get_measurement_value(Some("temperature"), "value"),
            Some(32.5)
        );
        assert_eq!(
            message_batch.get_measurement_value(Some("pressure"), "value"),
            Some(98.2)
        );
        assert_eq!(
            message_batch.get_measurement_value(Some("coordinate"), "x"),
            Some(50.0)
        );
        assert_eq!(
            message_batch.get_measurement_value(Some("coordinate"), "y"),
            Some(70.0)
        );
        assert_eq!(
            message_batch.get_measurement_value(Some("coordinate"), "z"),
            Some(90.0)
        );
    }

    #[test]
    fn invalid_collectd_topic_format() {
        let mut message_batch = MeasurementGrouper::new();

        let topic = Topic::new("collectd/less/level").unwrap();
        let collectd_message = Message::new(&topic, "123456789:32.5");
        let result = tedge_json_from_collectd_message(&mut message_batch, collectd_message);

        assert_matches!(result, Err(Error::InvalidMeasurementTopic(_)));
    }

    #[test]
    fn invalid_collectd_message_format() {
        let mut message_batch = MeasurementGrouper::new();

        let topic = Topic::new("collectd/host/group/key").unwrap();
        let invalid_collectd_message = Message::new(&topic, "123456789");
        let result = tedge_json_from_collectd_message(&mut message_batch, invalid_collectd_message);

        assert_matches!(result, Err(Error::InvalidMeasurementPayloadFormat(_, _)));
    }

    #[test]
    fn invalid_collectd_metric_value() {
        let mut message_batch = MeasurementGrouper::new();

        let topic = Topic::new("collectd/host/measurement/value").unwrap();
        let non_numeric_metric_value = Message::new(&topic, "123456789:abc");
        let result = tedge_json_from_collectd_message(&mut message_batch, non_numeric_metric_value);

        assert_matches!(result, Err(Error::InvalidMeasurementValue(_, _)));
    }

    #[test]
    fn very_large_metric_value() {
        let mut message_batch = MeasurementGrouper::new();

        let topic = Topic::new("collectd/host/measurement/value").unwrap();
        let non_numeric_metric_value = Message::new(&topic, format!("123456789:{}", MAX));
        let _ = tedge_json_from_collectd_message(&mut message_batch, non_numeric_metric_value);

        assert_eq!(
            message_batch
                .get_measurement_value(Some("measurement"), "value")
                .unwrap(),
            MAX as f64
        );
    }

    #[test]
    fn very_small_metric_value() {
        let mut message_batch = MeasurementGrouper::new();

        let topic = Topic::new("collectd/host/measurement/value").unwrap();
        let non_numeric_metric_value = Message::new(&topic, format!("123456789:{}", MIN));
        let _ = tedge_json_from_collectd_message(&mut message_batch, non_numeric_metric_value);

        assert_eq!(
            message_batch
                .get_measurement_value(Some("measurement"), "value")
                .unwrap(),
            MIN as f64
        );
    }
}
