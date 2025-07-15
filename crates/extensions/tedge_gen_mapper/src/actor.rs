use crate::flow::DateTime;
use crate::flow::Message;
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
                                self.processor.reload_filter(path).await;
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
                            if matches!(path.extension(), Some("js" | "ts")) {
                                self.processor.remove_filter(path).await;
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

    async fn filter(&mut self, message: Message) -> Result<(), RuntimeError> {
        let timestamp = DateTime::now();
        for (flow_id, flow_messages) in self.processor.process(&timestamp, &message).await {
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
                                error!(target: "gen-mapper", "{flow_id}: cannot send transformed message: {err}")
                            }
                        }
                    }
                }
                Err(err) => {
                    error!(target: "gen-mapper", "{flow_id}: {err}");
                }
            }
        }

        Ok(())
    }

    async fn tick(&mut self) -> Result<(), RuntimeError> {
        let timestamp = DateTime::now();
        if timestamp.seconds % 300 == 0 {
            self.processor.dump_memory_stats().await;
            self.processor.dump_processing_stats().await;
        }
        for (flow_id, flow_messages) in self.processor.tick(&timestamp).await {
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
                                error!(target: "gen-mapper", "{flow_id}: cannot send transformed message: {err}")
                            }
                        }
                    }
                }
                Err(err) => {
                    error!(target: "gen-mapper", "{flow_id}: {err}");
                }
            }
        }

        Ok(())
    }
}
