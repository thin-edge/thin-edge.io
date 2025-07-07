use crate::flow::DateTime;
use crate::flow::FlowOutput;
use crate::flow::Message;
use crate::runtime::MessageProcessor;
use crate::InputMessage;
use crate::OutputMessage;
use async_trait::async_trait;
use camino::Utf8PathBuf;
use futures::future::Either;
use std::future::pending;
use tedge_actors::Actor;
use tedge_actors::MessageReceiver;
use tedge_actors::RuntimeError;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use tedge_file_system_ext::FsWatchEvent;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::SubscriptionDiff;
use tedge_mqtt_ext::TopicFilter;
use tokio::time::sleep_until;
use tokio::time::Instant;
use tracing::error;

pub struct FlowsMapper {
    pub(super) messages: SimpleMessageBox<InputMessage, OutputMessage>,
    pub(super) subscriptions: TopicFilter,
    pub(super) processor: MessageProcessor,
}

#[async_trait]
impl Actor for FlowsMapper {
    fn name(&self) -> &str {
        "FlowsMapper"
    }

    async fn run(mut self) -> Result<(), RuntimeError> {
        loop {
            let deadline_future = match self.processor.next_interval_deadline() {
                Some(deadline) => Either::Left(sleep_until(deadline)),
                None => Either::Right(pending()),
            };

            tokio::select! {
                _ = deadline_future => {
                    self.on_interval().await?;

                    let drained_messages = self.drain_db().await?;
                    self.filter_all(drained_messages).await?;
                }
                message = self.messages.recv() => {
                    match message {
                        Some(InputMessage::MqttMessage(message)) => match Message::try_from(message) {
                            Ok(message) => self.on_message(message).await?,
                            Err(err) => {
                                error!(target: "flows", "Cannot process message: {err}");
                            }
                        },
                        Some(InputMessage::FsWatchEvent(FsWatchEvent::Modified(path))) => {
                            let Ok(path) = Utf8PathBuf::try_from(path) else {
                                continue;
                            };
                            if matches!(path.extension(), Some("js" | "ts" | "mjs")) {
                                self.processor.reload_script(path).await;
                            } else if path.extension() == Some("toml") {
                                self.processor.reload_flow(path).await;
                                self.send_updated_subscriptions().await?;
                            }
                        },
                        Some(InputMessage::FsWatchEvent(FsWatchEvent::FileCreated(path))) => {
                            let Ok(path) = Utf8PathBuf::try_from(path) else {
                                continue;
                            };
                            if matches!(path.extension(), Some("toml")) {
                                self.processor.add_flow(path).await;
                                self.send_updated_subscriptions().await?;
                            }
                        },
                        Some(InputMessage::FsWatchEvent(FsWatchEvent::FileDeleted(path))) => {
                            let Ok(path) = Utf8PathBuf::try_from(path) else {
                                continue;
                            };
                            if matches!(path.extension(), Some("js" | "ts" | "mjs")) {
                                self.processor.remove_script(path).await;
                            } else if path.extension() == Some("toml") {
                                self.processor.remove_flow(path).await;
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

impl FlowsMapper {
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

    async fn on_message(&mut self, message: Message) -> Result<(), RuntimeError> {
        let timestamp = DateTime::now();
        for (flow_id, flow_messages) in self.processor.on_message(&timestamp, &message).await {
            match flow_messages {
                Ok(messages) => {
                    for message in messages {
                        match MqttMessage::try_from(message) {
                            Ok(message) => {
                                self.messages
                                    .send(OutputMessage::MqttMessage(message))
                                    .await?
                            }
                            Err(err) => {
                                error!(target: "flows", "{flow_id}: cannot send transformed message: {err}")
                            }
                        }
                    }
                }
                Err(err) => {
                    error!(target: "flows", "{flow_id}: {err}");
                }
            }
        }

        Ok(())
    }

    async fn filter_all(&mut self, messages: Vec<(DateTime, Message)>) -> Result<(), RuntimeError> {
        for (timestamp, message) in messages {
            self.filter(timestamp, message).await?
        }
        Ok(())
    }

    async fn filter(&mut self, timestamp: DateTime, message: Message) -> Result<(), RuntimeError> {
        for (flow_id, flow_messages) in self.processor.process(&timestamp, &message).await {
            match flow_messages {
                Ok(messages) => {
                    self.publish_messages(flow_id.clone(), timestamp.clone(), messages)
                        .await?;
                }
                Err(err) => {
                    error!(target: "flows", "{flow_id}: {err}");
                }
            }
        }

        Ok(())
    }

    async fn on_interval(&mut self) -> Result<(), RuntimeError> {
        let now = Instant::now();
        let timestamp = DateTime::now();
        if timestamp.seconds % 300 == 0 {
            self.processor.dump_memory_stats().await;
            self.processor.dump_processing_stats().await;
        }
        for (flow_id, flow_messages) in self.processor.on_interval(&timestamp, now).await {
            match flow_messages {
                Ok(messages) => {
                    self.publish_messages(flow_id.clone(), timestamp.clone(), messages)
                        .await?;
                }
                Err(err) => {
                    error!(target: "flows", "{flow_id}: {err}");
                }
            }
        }

        Ok(())
    }

    async fn publish_messages(
        &mut self,
        flow_id: String,
        timestamp: DateTime,
        messages: Vec<Message>,
    ) -> Result<(), RuntimeError> {
        if let Some(flow) = self.processor.flows.get(&flow_id) {
            match &flow.output {
                FlowOutput::MQTT { output_topics } => {
                    for message in messages {
                        match MqttMessage::try_from(message) {
                            Ok(message) if output_topics.accept_topic(&message.topic) => {
                                self.messages
                                    .send(OutputMessage::MqttMessage(message))
                                    .await?
                            }
                            Ok(message) => {
                                error!(target: "flows", "{flow_id}: reject out-of-scope message: {}", message.topic)
                            }
                            Err(err) => {
                                error!(target: "flows", "{flow_id}: cannot send transformed message: {err}")
                            }
                        }
                    }
                }
                FlowOutput::MeaDB { output_series } => {
                    for message in messages {
                        if let Err(err) = self
                            .processor
                            .database
                            .store(output_series, timestamp, message)
                            .await
                        {
                            error!(target: "flows", "{flow_id}: fail to persist message: {err}");
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
        for (flow_id, flow_messages) in self.processor.drain_db(&timestamp).await {
            match flow_messages {
                Ok(flow_messages) => {
                    messages.extend(flow_messages);
                }
                Err(err) => {
                    error!(target: "flows", "{flow_id}: {err}");
                }
            }
        }
        Ok(messages)
    }
}
