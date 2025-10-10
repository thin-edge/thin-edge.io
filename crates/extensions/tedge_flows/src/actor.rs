use crate::flow::DateTime;
use crate::flow::Flow;
use crate::flow::FlowError;
use crate::flow::Message;
use crate::runtime::MessageProcessor;
use crate::InputMessage;
use crate::Tick;
use async_trait::async_trait;
use camino::Utf8PathBuf;
use futures::FutureExt;
use std::cmp::min;
use std::collections::HashSet;
use std::time::Duration;
use tedge_actors::Actor;
use tedge_actors::CloneSender;
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
use tracing::warn;

pub const STATS_DUMP_INTERVAL: Duration = Duration::from_secs(300);

pub struct FlowsMapper {
    pub(super) messages: SimpleMessageBox<InputMessage, SubscriptionDiff>,
    pub(super) mqtt_sender: DynSender<MqttMessage>,
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
                InputMessage::WatchEvent(WatchEvent::StdoutLine { topic, line }) => {
                    self.on_message(Message::new(topic, line)).await?
                }
                InputMessage::WatchEvent(WatchEvent::StderrLine { topic, line }) => {
                    warn!(target: "flows", "Input command {topic}: {line}");
                }
                InputMessage::WatchEvent(WatchEvent::Error { error, .. }) => {
                    error!(target: "flows", "Cannot monitor command: {error}");
                }
                InputMessage::WatchEvent(WatchEvent::EndOfStream { topic }) => {
                    error!(target: "flows", "End of input stream: {topic}");
                    self.on_input_eos(&topic).await?
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

    fn mqtt_sender(&self) -> DynSender<MqttMessage> {
        self.mqtt_sender.sender_clone()
    }

    async fn send_updated_subscriptions(&mut self) -> Result<(), RuntimeError> {
        let diff = self.update_subscriptions();
        self.messages.send(diff).await?;

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
        for (flow, messages) in self.processor.on_message(timestamp, &message).await {
            self.publish_transformation_outcome(&flow, messages).await?;
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
        for (flow, messages) in self.processor.on_interval(timestamp, now).await {
            self.publish_transformation_outcome(&flow, messages).await?;
        }

        Ok(())
    }

    async fn on_input_eos(&mut self, flow_name: &str) -> Result<(), RuntimeError> {
        if let Some(flow) = self.processor.flows.get(flow_name) {
            if let Some(request) = flow.watch_request() {
                info!(target: "flows", "Reconnecting input: {flow_name}: {}", flow.input);
                self.watch_request_sender.send(request).await?
            };
        }

        Ok(())
    }

    async fn publish_transformation_outcome(
        &mut self,
        flow_id: &str,
        transformation_outcome: Result<Vec<Message>, FlowError>,
    ) -> Result<(), RuntimeError> {
        let Some(flow) = self.processor.flows.get(flow_id) else {
            return Ok(());
        };
        match transformation_outcome {
            Ok(messages) => self.publish_transformed_messages(flow, messages).await,
            Err(err) => self.publish_transformation_error(flow, err).await,
        }
    }

    async fn publish_transformed_messages(
        &self,
        flow: &Flow,
        messages: Vec<Message>,
    ) -> Result<(), RuntimeError> {
        if let Err(err) = flow
            .output
            .publish_messages(flow.name(), self.mqtt_sender(), messages)
            .await
        {
            error!(target: "flows", "{}: cannot publish transformed message: {err}", flow.name());
        }

        Ok(())
    }

    async fn publish_transformation_error(
        &self,
        flow: &Flow,
        error: FlowError,
    ) -> Result<(), RuntimeError> {
        let message = Message::new("te/error".to_string(), error.to_string());
        if let Err(err) = flow
            .errors
            .publish_messages(flow.name(), self.mqtt_sender(), vec![message])
            .await
        {
            error!(target: "flows", "{}: cannot publish transformation error: {err}", flow.name());
        }

        Ok(())
    }
}
