use crate::js_filter::JsRuntime;
use crate::pipeline::DateTime;
use crate::pipeline::Message;
use crate::pipeline::Pipeline;
use async_trait::async_trait;
use std::collections::HashMap;
use tedge_actors::Actor;
use tedge_actors::MessageReceiver;
use tedge_actors::RuntimeError;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use tedge_mqtt_ext::MqttMessage;
use tokio::time::interval;
use tokio::time::Duration;
use tracing::error;

pub struct GenMapper {
    pub(super) mqtt: SimpleMessageBox<MqttMessage, MqttMessage>,
    pub(super) pipelines: HashMap<String, Pipeline>,
    pub(super) js_runtime: JsRuntime,
}

#[async_trait]
impl Actor for GenMapper {
    fn name(&self) -> &str {
        "GenMapper"
    }

    async fn run(mut self) -> Result<(), RuntimeError> {
        let mut interval = interval(Duration::from_secs(1));

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    self.tick().await?;
                }
                message = self.mqtt.recv() => {
                    match message.map(Message::try_from) {
                        Some(Ok(message)) => self.filter(message).await?,
                        Some(Err(err)) => {
                            error!(target: "gen-mapper", "Cannot process message: {err}");
                        },
                        None => break,
                    }
                }
            }
        }
        Ok(())
    }
}

impl GenMapper {
    async fn filter(&mut self, message: Message) -> Result<(), RuntimeError> {
        let timestamp = DateTime::now();
        for (pipeline_id, pipeline) in self.pipelines.iter_mut() {
            match pipeline
                .process(&self.js_runtime, &timestamp, &message)
                .await
            {
                Ok(messages) => {
                    for message in messages {
                        match MqttMessage::try_from(message) {
                            Ok(message) => self.mqtt.send(message).await?,
                            Err(err) => {
                                error!(target: "gen-mapper", "{pipeline_id}: cannot send transformed message: {err}")
                            }
                        }
                    }
                }
                Err(err) => {
                    error!(target: "gen-mapper", "{pipeline_id}: {err}");
                }
            }
        }

        Ok(())
    }

    async fn tick(&mut self) -> Result<(), RuntimeError> {
        let timestamp = DateTime::now();
        for (pipeline_id, pipeline) in self.pipelines.iter_mut() {
            match pipeline.tick(&self.js_runtime, &timestamp).await {
                Ok(messages) => {
                    for message in messages {
                        match MqttMessage::try_from(message) {
                            Ok(message) => self.mqtt.send(message).await?,
                            Err(err) => {
                                error!(target: "gen-mapper", "{pipeline_id}: cannot send transformed message: {err}")
                            }
                        }
                    }
                }
                Err(err) => {
                    error!(target: "gen-mapper", "{pipeline_id}: {err}");
                }
            }
        }

        Ok(())
    }
}
