use async_trait::async_trait;
use mqtt_plugin::MqttMessage;
use tedge_actors::{Actor, DevNull, Reactor, Recipient, RuntimeError, Task};
use telemetry_plugin::MeasurementGroup;
use thin_edge_json::group::MeasurementGrouperError;
use thin_edge_json::parser::ThinEdgeJsonParserError;

/// An actor that reads measurements published using ThinEdgeJson over MQTT
pub struct ThinEdgeJson {}

#[async_trait]
impl Actor for ThinEdgeJson {
    type Config = ();
    type Input = MqttMessage;
    type Output = MeasurementGroup;
    type Producer = DevNull;
    type Reactor = Self;

    fn try_new(_config: Self::Config) -> Result<Self, RuntimeError> {
        Ok(ThinEdgeJson {})
    }

    async fn start(self) -> Result<(Self::Producer, Self::Reactor), RuntimeError> {
        Ok((DevNull, self))
    }
}

#[async_trait]
impl Reactor<MqttMessage, MeasurementGroup> for ThinEdgeJson {
    async fn react(
        &mut self,
        message: MqttMessage,
        output: &mut Recipient<MeasurementGroup>,
    ) -> Result<Option<Box<dyn Task>>, RuntimeError> {
        if let Ok(measurements) = ThinEdgeJson::parse(message.payload) {
            let _ = output.send_message(measurements).await;
        }
        Ok(None)
    }
}

impl ThinEdgeJson {
    // This code is a bit clumsy because it avoids on purpose to reuse the type `thin_edge_json::group::Measurement`
    // And having then to transform such values into `telemetry_plugin::Measurement` values.
    pub fn parse(input: String) -> Result<MeasurementGroup, ThinEdgeJsonError> {
        let mut builder = thin_edge_json::group::MeasurementGrouper::new();
        let () = thin_edge_json::parser::parse_str(&input, &mut builder)?;

        let group = builder.end()?;
        let timestamp = group.timestamp;
        let values = group
            .values
            .into_iter()
            .map(|(k, v)| {
                (
                    k,
                    match v {
                        thin_edge_json::group::Measurement::Single(m) => {
                            telemetry_plugin::Measurement::Single(m)
                        }
                        thin_edge_json::group::Measurement::Multi(ms) => {
                            telemetry_plugin::Measurement::Multi(ms)
                        }
                    },
                )
            })
            .collect();

        Ok(MeasurementGroup { timestamp, values })
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ThinEdgeJsonError {
    #[error(transparent)]
    MeasurementGrouperError(#[from] MeasurementGrouperError),

    #[error(transparent)]
    ThinEdgeJsonParserError(#[from] ThinEdgeJsonParserError),
}
