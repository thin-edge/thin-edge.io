use async_trait::async_trait;
use c8y_translator::json::CumulocityJsonError;
use mqtt_plugin::MqttMessage;
use tedge_actors::{Actor, DevNull, Reactor, Recipient, RuntimeError};
use telemetry_plugin::{Measurement, MeasurementGroup};
use thin_edge_json::measurement::MeasurementVisitor;
use time::OffsetDateTime;

/// An actor that establishes the connection between the device and a Cumulocity instance
pub struct C8Y {
    c8y_measurement_topic: String,
}

#[async_trait]
impl Actor for C8Y {
    type Config = String;
    type Input = MeasurementGroup;
    type Output = MqttMessage;
    type Producer = DevNull;
    type Reactor = Self;

    fn try_new(config: &Self::Config) -> Result<Self, RuntimeError> {
        Ok(C8Y {
            c8y_measurement_topic: config.clone(),
        })
    }

    async fn start(self) -> Result<(Self::Producer, Self::Reactor), RuntimeError> {
        Ok((DevNull, self))
    }
}

#[async_trait]
impl Reactor<MeasurementGroup, MqttMessage> for C8Y {
    async fn react(
        &mut self,
        measurements: MeasurementGroup,
        output: &mut impl Recipient<MqttMessage>,
    ) -> Result<(), RuntimeError> {
        if let Ok(c8y_payload) = C8Y::serialize(measurements) {
            let _ = output
                .send_message(MqttMessage {
                    topic: self.c8y_measurement_topic.clone(),
                    payload: c8y_payload,
                })
                .await;
        }
        Ok(())
    }
}

impl C8Y {
    fn serialize(measurements: MeasurementGroup) -> Result<String, CumulocityJsonError> {
        let timestamp = OffsetDateTime::now_utc();
        let mut serializer = c8y_translator::serializer::C8yJsonSerializer::new(timestamp, None);

        if let Some(t) = measurements.timestamp {
            serializer.visit_timestamp(t)?;
        }
        for (key, measurement) in measurements.values.iter() {
            match measurement {
                Measurement::Single(value) => {
                    serializer.visit_measurement(key, *value)?;
                }
                Measurement::Multi(values) => {
                    serializer.visit_start_group(key)?;
                    for (key, value) in values.iter() {
                        serializer.visit_measurement(key, *value)?;
                    }
                    serializer.visit_end_group()?;
                }
            }
        }

        let output = serializer.into_string()?;
        Ok(output)
    }
}
