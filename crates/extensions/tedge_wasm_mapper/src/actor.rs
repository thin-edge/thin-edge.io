use crate::pipeline::Pipeline;
use async_trait::async_trait;
use std::collections::HashMap;
use tedge_actors::Actor;
use tedge_actors::MessageReceiver;
use tedge_actors::RuntimeError;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use tedge_mqtt_ext::MqttMessage;
use time::OffsetDateTime;
use tokio::time::interval;
use tokio::time::Duration;
use tracing::error;

pub struct WasmMapper {
    pub(super) mqtt: SimpleMessageBox<MqttMessage, MqttMessage>,
    pub(super) pipelines: HashMap<String, Pipeline>,
}

#[async_trait]
impl Actor for WasmMapper {
    fn name(&self) -> &str {
        "WasmMapper"
    }

    async fn run(mut self) -> Result<(), RuntimeError> {
        let mut interval = interval(Duration::from_secs(5));

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    self.tick().await?;
                }
                message = self.mqtt.recv() => {
                    match message {
                        Some(message) => self.filter(message).await?,
                        None => break,
                    }
                }
            }
        }
        Ok(())
    }
}

impl WasmMapper {
    async fn filter(&mut self, message: MqttMessage) -> Result<(), RuntimeError> {
        let timestamp = OffsetDateTime::now_utc();
        for (pipeline_id, pipeline) in self.pipelines.iter_mut() {
            match pipeline.process(timestamp, &message) {
                Ok(messages) => {
                    for message in messages {
                        self.mqtt.send(message).await?;
                    }
                }
                Err(err) => {
                    error!(target: "wasm-mapper", "{pipeline_id}: {err}");
                }
            }
        }

        Ok(())
    }

    async fn tick(&mut self) -> Result<(), RuntimeError> {
        let timestamp = OffsetDateTime::now_utc();
        for (pipeline_id, pipeline) in self.pipelines.iter_mut() {
            match pipeline.tick(timestamp) {
                Ok(messages) => {
                    for message in messages {
                        self.mqtt.send(message).await?;
                    }
                }
                Err(err) => {
                    error!(target: "wasm-mapper", "{pipeline_id}: {err}");
                }
            }
        }

        Ok(())
    }
}
