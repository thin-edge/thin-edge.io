use crate::config::PipelineConfig;
use crate::js_filter::JsRuntime;
use crate::pipeline::DateTime;
use crate::pipeline::Message;
use crate::pipeline::Pipeline;
use crate::InputMessage;
use crate::OutputMessage;
use async_trait::async_trait;
use camino::Utf8PathBuf;
use std::collections::HashMap;
use std::path::PathBuf;
use tedge_actors::Actor;
use tedge_actors::MessageReceiver;
use tedge_actors::RuntimeError;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use tedge_file_system_ext::FsWatchEvent;
use tedge_mqtt_ext::MqttMessage;
use tokio::time::interval;
use tokio::time::Duration;
use tracing::error;
use tracing::info;

pub struct GenMapper {
    pub(super) messages: SimpleMessageBox<InputMessage, OutputMessage>,
    pub(super) pipelines: HashMap<String, Pipeline>,
    pub(super) js_runtime: JsRuntime,
    pub(super) config_dir: PathBuf,
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
                message = self.messages.recv() => {
                    match message {
                        Some(InputMessage::MqttMessage(message)) => match Message::try_from(message) {
                            Ok(message) => self.filter(message).await?,
                            Err(err) => {
                                error!(target: "gen-mapper", "Cannot process message: {err}");
                            }
                        },
                        Some(InputMessage::FsWatchEvent(FsWatchEvent::Modified(path))) => {
                            let Ok(path) = Utf8PathBuf::try_from(path) else {
                                continue;
                            };
                            if matches!(path.extension(), Some("js" | "ts")) {
                                self.reload_filter(path).await;
                            } else if path.extension() == Some("toml") {
                                self.reload_pipeline(path).await;
                            }
                        },
                        Some(InputMessage::FsWatchEvent(e)) => {
                            tracing::warn!("TODO do something with {e:?}")
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
    async fn reload_filter(&mut self, path: Utf8PathBuf) {
        for pipeline in self.pipelines.values_mut() {
            for stage in &mut pipeline.stages {
                if stage.filter.path() == path {
                    match self.js_runtime.load_file(&path) {
                        Ok(filter) => {
                            info!("Reloaded filter {path}");
                            stage.filter = filter
                        }
                        Err(e) => {
                            error!("Failed to reload filter {path}: {e}");
                            return;
                        }
                    }
                }
            }
        }
    }

    async fn reload_pipeline(&mut self, path: Utf8PathBuf) {
        for pipeline in self.pipelines.values_mut() {
            if pipeline.source == path {
                let Ok(source) = tokio::fs::read_to_string(&path).await else {
                    error!("Failed to read updated filter {path}");
                    break;
                };
                let config: PipelineConfig = match toml::from_str(&source) {
                    Ok(config) => config,
                    Err(e) => {
                        error!("Failed to parse toml for updated filter {path}: {e}");
                        break;
                    }
                };
                match config.compile(&self.js_runtime, &self.config_dir, path.clone()) {
                    Ok(p) => {
                        *pipeline = p;
                        self.messages
                            .send(OutputMessage::TopicFilter(pipeline.input_topics.clone()))
                            .await
                            .unwrap();
                        info!("Reloaded pipeline {path}");
                    }
                    Err(e) => {
                        error!("Failed to load updated pipeline {path}: {e}")
                    }
                };
            }
        }
    }

    async fn filter(&mut self, message: Message) -> Result<(), RuntimeError> {
        let timestamp = DateTime::now();
        for (pipeline_id, pipeline) in self.pipelines.iter_mut() {
            match pipeline.process(&self.js_runtime, &timestamp, &message) {
                Ok(messages) => {
                    for message in messages {
                        match MqttMessage::try_from(message) {
                            Ok(message) => {
                                self.messages
                                    .send(OutputMessage::MqttMessage(message))
                                    .await?
                            }
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
        for (pipeline_id, pipeline) in self.pipelines.iter() {
            match pipeline.tick(&self.js_runtime, &timestamp) {
                Ok(messages) => {
                    for message in messages {
                        match MqttMessage::try_from(message) {
                            Ok(message) => {
                                self.messages
                                    .send(OutputMessage::MqttMessage(message))
                                    .await?
                            }
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
