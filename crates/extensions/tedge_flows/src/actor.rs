use crate::flow::DateTime;
use crate::flow::Message;
use crate::runtime::MessageProcessor;
use crate::InputMessage;
use crate::OutputMessage;
use crate::Tick;
use async_trait::async_trait;
use camino::Utf8PathBuf;
use futures::FutureExt;
use std::cmp::min;
use std::collections::HashSet;
use std::time::Duration;
use tedge_actors::Actor;
use tedge_actors::DynSender;
use tedge_actors::MessageReceiver;
use tedge_actors::RuntimeError;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use tedge_file_system_ext::FsWatchEvent;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::SubscriptionDiff;
use tedge_mqtt_ext::TopicFilter;
use tedge_watch_ext::WatchEvent;
use tedge_watch_ext::WatchRequest;
use tokio::time::sleep_until;
use tokio::time::Instant;
use tracing::error;
use tracing::info;

pub const STATS_DUMP_INTERVAL: Duration = Duration::from_secs(300);

pub struct FlowsMapper {
    pub(super) messages: SimpleMessageBox<InputMessage, OutputMessage>,
    pub(super) watch_request_sender: DynSender<WatchRequest>,
    pub(super) subscriptions: TopicFilter,
    pub(super) watched_commands: HashSet<String>,
    pub(super) processor: MessageProcessor,
    pub(super) next_dump: Instant,
}

#[async_trait]
impl Actor for FlowsMapper {
    fn name(&self) -> &str {
        "FlowsMapper"
    }

    async fn run(mut self) -> Result<(), RuntimeError> {
        self.send_updated_subscriptions().await?;

        while let Some(message) = self.next_message().await {
            match message {
                InputMessage::Tick(_) => {
                    self.on_interval().await?;
                }
                InputMessage::MqttMessage(message) => {
                    self.on_message(Message::from(message)).await?
                }
                InputMessage::WatchEvent(WatchEvent::NewLine { topic, line }) => {
                    self.on_message(Message::new(topic, line)).await?
                }
                InputMessage::WatchEvent(WatchEvent::Error { error, .. }) => {
                    error!(target: "flows", "Cannot monitor command: {error}");
                }
                InputMessage::WatchEvent(WatchEvent::EndOfStream { topic }) => {
                    error!(target: "flows", "End of input stream: {topic}");
                }
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
                _ => continue,
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

        for watch_request in self.update_watched_commands() {
            self.watch_request_sender.send(watch_request).await?;
        }
        Ok(())
    }

    fn update_subscriptions(&mut self) -> SubscriptionDiff {
        let new_subscriptions = self.processor.subscriptions();
        let diff = SubscriptionDiff::new(&new_subscriptions, &self.subscriptions);
        self.subscriptions = new_subscriptions;
        diff
    }

    fn update_watched_commands(&mut self) -> Vec<WatchRequest> {
        let mut watch_requests = Vec::new();
        let mut new_watched_commands = HashSet::new();
        for flow in self.processor.flows.values() {
            let topic = flow.name();
            let Some(request) = flow.watch_request() else {
                continue;
            };
            if !self.watched_commands.contains(topic) {
                info!(target: "flows", "Adding input: {}", flow.input);
                watch_requests.push(request);
            }
            self.watched_commands.remove(topic);
            new_watched_commands.insert(topic.to_owned());
        }
        for old_command in self.watched_commands.drain() {
            info!(target: "flows", "removing input: {}", old_command);
            watch_requests.push(WatchRequest::UnWatch { topic: old_command });
        }
        self.watched_commands = new_watched_commands;
        watch_requests
    }

    async fn on_message(&mut self, message: Message) -> Result<(), RuntimeError> {
        let timestamp = DateTime::now();
        for (flow_id, flow_messages) in self.processor.on_message(timestamp, &message).await {
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
}
