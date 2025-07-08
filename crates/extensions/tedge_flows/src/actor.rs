use crate::flow::DateTime;
use crate::flow::FlowOutput;
use crate::flow::Message;
use crate::flow::MessageSource;
use crate::runtime::MessageProcessor;
use crate::InputMessage;
use crate::OutputMessage;
use crate::Tick;
use async_trait::async_trait;
use camino::Utf8PathBuf;
use futures::FutureExt;
use std::cmp::min;
use std::time::Duration;
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
use tracing::info;

pub const STATS_DUMP_INTERVAL: Duration = Duration::from_secs(300);

pub struct FlowsMapper {
    pub(super) messages: SimpleMessageBox<InputMessage, OutputMessage>,
    pub(super) subscriptions: TopicFilter,
    pub(super) processor: MessageProcessor,
    pub(super) next_dump: Instant,
}

#[async_trait]
impl Actor for FlowsMapper {
    fn name(&self) -> &str {
        "FlowsMapper"
    }

    async fn run(mut self) -> Result<(), RuntimeError> {
        while let Some(message) = self.next_message().await {
            match message {
                InputMessage::Tick(_) => {
                    let drained_messages = self.drain_db().await?;
                    self.on_messages(MessageSource::MeaDB, drained_messages)
                        .await?;

                    self.on_interval().await?;
                }
                InputMessage::MqttMessage(message) => match Message::try_from(message) {
                    Ok(message) => {
                        self.on_message(MessageSource::MQTT, DateTime::now(), message)
                            .await?
                    }
                    Err(err) => {
                        error!(target: "flows", "Cannot process message: {err}");
                    }
                },
                InputMessage::FsWatchEvent(FsWatchEvent::Modified(path)) => {
                    let Ok(path) = Utf8PathBuf::try_from(path) else {
                        continue;
                    };
                    if matches!(path.extension(), Some("js" | "ts" | "mjs")) {
                        self.processor.reload_script(path).await;
                    } else if path.extension() == Some("toml") {
                        self.processor.reload_flow(path).await;
                        self.send_updated_subscriptions().await?;
                    }
                }
                InputMessage::FsWatchEvent(FsWatchEvent::FileCreated(path)) => {
                    let Ok(path) = Utf8PathBuf::try_from(path) else {
                        continue;
                    };
                    if matches!(path.extension(), Some("toml")) {
                        self.processor.add_flow(path).await;
                        self.send_updated_subscriptions().await?;
                    }
                }
                InputMessage::FsWatchEvent(FsWatchEvent::FileDeleted(path)) => {
                    let Ok(path) = Utf8PathBuf::try_from(path) else {
                        continue;
                    };
                    if matches!(path.extension(), Some("js" | "ts" | "mjs")) {
                        self.processor.remove_script(path).await;
                    } else if path.extension() == Some("toml") {
                        self.processor.remove_flow(path).await;
                        self.send_updated_subscriptions().await?;
                    }
                }
                InputMessage::FsWatchEvent(_) => unimplemented!(),
            }
        }

        Ok(())
    }
}

impl FlowsMapper {
    async fn next_message(&mut self) -> Option<InputMessage> {
        let deadline = self
            .processor
            .next_interval_deadline()
            .map_or(self.next_dump, |deadline| min(deadline, self.next_dump));
        let deadline_future = sleep_until(deadline).map(|_| Some(InputMessage::Tick(Tick)));
        let incoming_message_future = self.messages.recv();

        futures::pin_mut!(incoming_message_future);
        futures::pin_mut!(deadline_future);

        futures::future::select(deadline_future, incoming_message_future)
            .await
            .factor_first()
            .0
    }

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

    async fn on_message(
        &mut self,
        source: MessageSource,
        timestamp: DateTime,
        message: Message,
    ) -> Result<(), RuntimeError> {
        for (flow_id, flow_messages) in self.processor.on_message(source, timestamp, &message).await
        {
            match flow_messages {
                Ok(messages) => self.publish_messages(flow_id, timestamp, messages).await?,
                Err(err) => {
                    error!(target: "flows", "{flow_id}: {err}");
                }
            }
        }

        Ok(())
    }

    async fn on_messages(
        &mut self,
        source: MessageSource,
        messages: Vec<(DateTime, Message)>,
    ) -> Result<(), RuntimeError> {
        for (timestamp, message) in messages {
            self.on_message(source, timestamp, message).await?
        }
        Ok(())
    }

    async fn on_interval(&mut self) -> Result<(), RuntimeError> {
        let now = Instant::now();
        let timestamp = DateTime::now();
        if self.next_dump <= now {
            self.processor.dump_memory_stats().await;
            self.processor.dump_processing_stats().await;
            self.next_dump = now + STATS_DUMP_INTERVAL;
        }
        for (flow_id, flow_messages) in self.processor.on_interval(timestamp, now).await {
            match flow_messages {
                Ok(messages) => {
                    self.publish_messages(flow_id.clone(), timestamp, messages)
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
                        info!(target: "flows", "store {output_series} @{}.{} [{}]", timestamp.seconds, timestamp.nanoseconds, message.topic);
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
        for (flow_id, flow_messages) in self.processor.drain_db(timestamp).await {
            match flow_messages {
                Ok(flow_messages) => {
                    for (t, m) in flow_messages.iter() {
                        info!(target: "flows", "drained: @{}.{} [{}]", t.seconds, t.nanoseconds, m.topic);
                    }
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
