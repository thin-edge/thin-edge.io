use crate::pipeline::DateTime;
use crate::pipeline::Message;
use crate::pipeline::MessageSource;
use crate::pipeline::PipelineOutput;
use crate::runtime::MessageProcessor;
use crate::InputMessage;
use crate::OutputMessage;
use async_trait::async_trait;
use camino::Utf8PathBuf;
use tedge_actors::Actor;
use tedge_actors::MessageReceiver;
use tedge_actors::RuntimeError;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use tedge_file_system_ext::FsWatchEvent;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::SubscriptionDiff;
use tedge_mqtt_ext::TopicFilter;
use tokio::time::interval;
use tokio::time::Duration;
use tracing::error;
use tracing::info;

pub struct GenMapper {
    pub(super) messages: SimpleMessageBox<InputMessage, OutputMessage>,
    pub(super) subscriptions: TopicFilter,
    pub(super) processor: MessageProcessor,
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

                    let drained_messages = self.drain_db().await?;
                    self.filter_all(MessageSource::MeaDB, drained_messages).await?;
                }
                message = self.messages.recv() => {
                    match message {
                        Some(InputMessage::MqttMessage(message)) => match Message::try_from(message) {
                            Ok(message) => self.filter(MessageSource::MQTT, DateTime::now(), message).await?,
                            Err(err) => {
                                error!(target: "gen-mapper", "Cannot process message: {err}");
                            }
                        },
                        Some(InputMessage::FsWatchEvent(FsWatchEvent::Modified(path))) => {
                            let Ok(path) = Utf8PathBuf::try_from(path) else {
                                continue;
                            };
                            if matches!(path.extension(), Some("js" | "ts")) {
                                self.processor.reload_filter(path).await;
                            } else if path.extension() == Some("toml") {
                                self.processor.reload_pipeline(path).await;
                                self.send_updated_subscriptions().await?;
                            }
                        },
                        Some(InputMessage::FsWatchEvent(FsWatchEvent::FileCreated(path))) => {
                            let Ok(path) = Utf8PathBuf::try_from(path) else {
                                continue;
                            };
                            if matches!(path.extension(), Some("toml")) {
                                self.processor.add_pipeline(path).await;
                                self.send_updated_subscriptions().await?;
                            }
                        },
                        Some(InputMessage::FsWatchEvent(FsWatchEvent::FileDeleted(path))) => {
                            let Ok(path) = Utf8PathBuf::try_from(path) else {
                                continue;
                            };
                            if matches!(path.extension(), Some("js" | "ts")) {
                                self.processor.remove_filter(path).await;
                            } else if path.extension() == Some("toml") {
                                self.processor.remove_pipeline(path).await;
                                self.send_updated_subscriptions().await?;
                            }
                        },
                        _ => break,
                    }
                }
            }
        }
        Ok(())
    }
}

impl GenMapper {
    async fn send_updated_subscriptions(&mut self) -> Result<(), RuntimeError> {
        let diff = self.update_subscriptions();
        self.messages
            .send(OutputMessage::SubscriptionDiff(diff))
            .await?;
        Ok(())
    }

    fn update_subscriptions(&mut self) -> SubscriptionDiff {
        let new_subscriptions = self.processor.subscriptions();
        let diff = SubscriptionDiff::new(&new_subscriptions, &self.subscriptions);
        self.subscriptions = new_subscriptions;
        diff
    }

    async fn filter_all(
        &mut self,
        source: MessageSource,
        messages: Vec<(DateTime, Message)>,
    ) -> Result<(), RuntimeError> {
        for (timestamp, message) in messages {
            self.filter(source, timestamp, message).await?
        }
        Ok(())
    }

    async fn filter(
        &mut self,
        source: MessageSource,
        timestamp: DateTime,
        message: Message,
    ) -> Result<(), RuntimeError> {
        for (pipeline_id, pipeline_messages) in
            self.processor.process(source, &timestamp, &message).await
        {
            match pipeline_messages {
                Ok(messages) => {
                    self.publish_messages(pipeline_id, timestamp.clone(), messages)
                        .await?;
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
        if timestamp.seconds % 300 == 0 {
            self.processor.dump_memory_stats().await;
        }
        for (pipeline_id, pipeline_messages) in self.processor.tick(&timestamp).await {
            match pipeline_messages {
                Ok(messages) => {
                    self.publish_messages(pipeline_id, timestamp.clone(), messages)
                        .await?;
                }
                Err(err) => {
                    error!(target: "gen-mapper", "{pipeline_id}: {err}");
                }
            }
        }

        Ok(())
    }

    async fn publish_messages(
        &mut self,
        pipeline_id: String,
        timestamp: DateTime,
        messages: Vec<Message>,
    ) -> Result<(), RuntimeError> {
        if let Some(pipeline) = self.processor.pipelines.get(&pipeline_id) {
            match &pipeline.output {
                PipelineOutput::MQTT {
                    topics: output_topics,
                } => {
                    for message in messages {
                        match MqttMessage::try_from(message) {
                            Ok(message) if output_topics.accept_topic(&message.topic) => {
                                self.messages
                                    .send(OutputMessage::MqttMessage(message))
                                    .await?
                            }
                            Ok(message) => {
                                error!(target: "gen-mapper", "{pipeline_id}: reject out-of-scope message: {}", message.topic)
                            }
                            Err(err) => {
                                error!(target: "gen-mapper", "{pipeline_id}: cannot send transformed message: {err}")
                            }
                        }
                    }
                }
                PipelineOutput::MeaDB {
                    series: output_series,
                } => {
                    for message in messages {
                        info!(target: "gen-mapper", "store {output_series} @{}.{} [{}] {}", timestamp.seconds, timestamp.nanoseconds, message.topic, message.payload);
                        if let Err(err) = self
                            .processor
                            .database
                            .store(output_series, timestamp.clone(), message)
                            .await
                        {
                            error!(target: "gen-mapper", "{pipeline_id}: fail to persist message: {err}");
                        }
                    }
                }
            }
        }

        Ok(())
    }

    async fn drain_db(&mut self) -> Result<Vec<(DateTime, Message)>, RuntimeError> {
        let timestamp = DateTime::now();
        let mut messages = vec![];
        for (pipeline_id, pipeline_messages) in self.processor.drain_db(&timestamp).await {
            match pipeline_messages {
                Ok(pipeline_messages) => {
                    for (t, m) in pipeline_messages.iter() {
                        info!(target: "gen-mapper", "drained: @{}.{} [{}] {}", t.seconds, t.nanoseconds, m.topic, m.payload);
                    }
                    messages.extend(pipeline_messages);
                }
                Err(err) => {
                    error!(target: "gen-mapper", "{pipeline_id}: {err}");
                }
            }
        }
        Ok(messages)
    }
}
