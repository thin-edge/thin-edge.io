use crate::connected_flow::ConnectedFlowRegistry;
use crate::flow::FlowError;
use crate::flow::FlowOutput;
use crate::flow::FlowResult;
use crate::flow::Message;
use crate::flow::SourceTag;
use crate::registry::FlowRegistryExt;
use crate::registry::RegistrationStatus;
use crate::runtime::MessageProcessor;
use crate::FlowsMapperConfig;
use crate::InputMessage;
use crate::Tick;
use async_trait::async_trait;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use futures::FutureExt;
use serde_json::json;
use std::cmp::min;
use std::collections::HashSet;
use std::time::Duration;
use std::time::SystemTime;
use tedge_actors::Actor;
use tedge_actors::DynSender;
use tedge_actors::MessageReceiver;
use tedge_actors::RuntimeError;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use tedge_file_system_ext::FsWatchEvent;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::QoS;
use tedge_mqtt_ext::SubscriptionDiff;
use tedge_mqtt_ext::TopicFilter;
use tedge_watch_ext::WatchEvent;
use tedge_watch_ext::WatchRequest;
use time::OffsetDateTime;
use tokio::io::AsyncWriteExt;
use tokio::time::sleep_until;
use tokio::time::Instant;
use tracing::error;
use tracing::info;
use tracing::warn;

pub struct FlowsMapper {
    config: FlowsMapperConfig,
    messages: SimpleMessageBox<InputMessage, SubscriptionDiff>,
    mqtt_sender: DynSender<MqttMessage>,
    watch_request_sender: DynSender<WatchRequest>,
    subscriptions: TopicFilter,
    watched_commands: HashSet<Utf8PathBuf>,
    processor: MessageProcessor<ConnectedFlowRegistry>,
    next_dump: Instant,
    /// Paths to flow declaration and script files that are currently in use.
    ///
    /// When a directory containing flows is moved from the flows directory, we only get an fs event
    /// containing the name of the directory, so we need to remember what files were loaded so we can unload them.
    loaded_files: HashSet<Utf8PathBuf>,
}

impl FlowsMapper {
    pub fn new(
        config: FlowsMapperConfig,
        messages: SimpleMessageBox<InputMessage, SubscriptionDiff>,
        mqtt_sender: DynSender<MqttMessage>,
        watch_request_sender: DynSender<WatchRequest>,
        subscriptions: TopicFilter,
        processor: MessageProcessor<ConnectedFlowRegistry>,
    ) -> Self {
        let watched_commands = HashSet::new();
        let next_dump = Instant::now() + config.stats_dump_interval;
        let loaded_files = HashSet::new();
        FlowsMapper {
            config,
            messages,
            mqtt_sender,
            watch_request_sender,
            subscriptions,
            watched_commands,
            processor,
            next_dump,
            loaded_files,
        }
    }
}

#[async_trait]
impl Actor for FlowsMapper {
    fn name(&self) -> &str {
        "FlowsMapper"
    }

    async fn run(mut self) -> Result<(), RuntimeError> {
        self.send_updated_subscriptions().await?;
        self.notify_flows_status().await?;

        while let Some(message) = self.next_message().await {
            match message {
                InputMessage::Tick(_) => {
                    self.on_source_poll().await?;
                    self.on_interval().await?;
                }
                InputMessage::MqttMessage(message) => {
                    let source = SourceTag::Mqtt;
                    self.on_message(source, Message::from(message)).await?
                }
                InputMessage::WatchEvent(event) => {
                    self.on_input_event(event).await?;
                }
                InputMessage::FsWatchEvent(event) => self.handle_fs_event(event).await?,
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
        for flow in self.processor.registry.flows() {
            let flow_path = flow.source_path();
            let Some(request) = flow.watch_request() else {
                continue;
            };
            if !self.watched_commands.contains(flow_path) {
                info!(target: "flows", "Adding input: {}", flow.as_ref().input);
                watch_requests.push(request);
            }
            self.watched_commands.remove(flow_path);
            new_watched_commands.insert(flow_path.to_owned());
        }
        for old_command in self.watched_commands.drain() {
            info!(target: "flows", "Removing input: {}", old_command);
            watch_requests.push(WatchRequest::UnWatch {
                topic: old_command.to_string(),
            });
        }
        self.watched_commands = new_watched_commands;
        watch_requests
    }

    async fn notify_flows_status(&mut self) -> Result<(), RuntimeError> {
        let status = "enabled";
        let now = OffsetDateTime::now_utc();
        for flow in self.processor.registry.flows() {
            let status = self.flow_status(flow.source_path(), status, &now);
            self.mqtt_sender.send(status).await?;
        }
        Ok(())
    }

    async fn update_all_flow_status(
        &mut self,
        flows: Vec<Utf8PathBuf>,
    ) -> Result<(), RuntimeError> {
        for flow in flows {
            self.update_flow_status(&flow).await?;
        }
        Ok(())
    }

    async fn update_flow_status(&mut self, flow: &Utf8Path) -> Result<(), RuntimeError> {
        let now = OffsetDateTime::now_utc();
        let status = match self.processor.registry.registration_status(flow) {
            RegistrationStatus::Unregistered => "removed",
            RegistrationStatus::Registered => "updated",
            RegistrationStatus::Broken => "broken",
        };
        let status = self.flow_status(flow, status, &now);
        self.mqtt_sender.send(status).await?;
        Ok(())
    }

    fn flow_status(&self, flow: &Utf8Path, status: &str, time: &OffsetDateTime) -> MqttMessage {
        let payload = json!({
            "flow": flow.as_str(),
            "status": status,
            "time": time.unix_timestamp(),
        });
        MqttMessage::new(&self.config.status_topic, payload.to_string()).with_qos(QoS::AtLeastOnce)
    }

    async fn on_source_poll(&mut self) -> Result<(), RuntimeError> {
        let now = Instant::now();
        let timestamp = SystemTime::now();

        let mut in_messages = vec![];
        for flow in self.processor.registry.flows_mut() {
            in_messages.push(flow.on_source_poll(timestamp, now).await);
        }

        for messages in in_messages {
            match messages {
                FlowResult::Ok { flow, messages, .. } => {
                    for message in messages {
                        if let Some(flow_output) = self
                            .processor
                            .on_flow_input(&flow, timestamp, &message)
                            .await
                        {
                            self.publish_result(flow_output).await?;
                        }
                    }
                }
                poll_error => {
                    self.publish_result(poll_error).await?;
                }
            }
        }

        Ok(())
    }

    async fn on_message(
        &mut self,
        source: SourceTag,
        message: Message,
    ) -> Result<(), RuntimeError> {
        let timestamp = SystemTime::now();
        for messages in self
            .processor
            .on_message(timestamp, &source, &message)
            .await
        {
            self.publish_result(messages).await?;
        }

        Ok(())
    }

    async fn on_interval(&mut self) -> Result<(), RuntimeError> {
        let now = Instant::now();
        let timestamp = SystemTime::now();
        if self.next_dump <= now {
            info!(target: "flows", "Collect memory usage and processing statistics");
            if let Some(record) = self
                .processor
                .dump_memory_stats(&self.config.stats_publisher)
                .await
            {
                self.mqtt_sender.send(record).await?;
            }
            for record in self
                .processor
                .dump_processing_stats(&self.config.stats_publisher, &self.config.stats_filter)
                .await
            {
                self.mqtt_sender.send(record).await?;
            }
            self.next_dump = now + self.config.stats_dump_interval;
        }
        for messages in self.processor.on_interval(timestamp, now).await {
            self.publish_result(messages).await?;
        }

        Ok(())
    }

    async fn on_input_event(&mut self, event: WatchEvent) -> Result<(), RuntimeError> {
        match event {
            WatchEvent::StdoutLine { topic, line } => {
                self.on_input_message(Utf8Path::new(&topic), line).await?;
            }
            WatchEvent::StderrLine { topic, line } => {
                warn!(target: "flows", "Input command {topic}: {line}");
            }
            WatchEvent::Error { topic, error } => {
                error!(target: "flows", "Cannot monitor command: {error}");
                self.on_input_error(Utf8Path::new(&topic), error.into())
                    .await?;
            }
            WatchEvent::EndOfStream { topic } => {
                error!(target: "flows", "End of input stream: {topic}");
                self.on_input_eos(Utf8Path::new(&topic)).await?
            }
        }
        Ok(())
    }

    async fn on_input_message(
        &mut self,
        flow_path: &Utf8Path,
        line: String,
    ) -> Result<(), RuntimeError> {
        if let Some(flow) = self.processor.registry.flow(flow_path) {
            let topic = flow.input_topic().to_string();
            let timestamp = SystemTime::now();
            let message = Message::new(topic, line);
            if let Some(result) = self
                .processor
                .on_flow_input(flow_path, timestamp, &message)
                .await
            {
                self.publish_result(result).await?;
            }
        }

        Ok(())
    }

    async fn on_input_error(
        &mut self,
        flow_path: &Utf8Path,
        error: FlowError,
    ) -> Result<(), RuntimeError> {
        let Some((info, flow_error)) = self.processor.registry.flow(flow_path).map(|flow| {
            (
                format!("Reconnecting input: {flow_path}: {}", flow.as_ref().input),
                flow.on_error(error),
            )
        }) else {
            return Ok(());
        };
        self.publish_result(flow_error).await?;

        let Some(request) = self
            .processor
            .registry
            .flow(flow_path)
            .and_then(|flow| flow.watch_request())
        else {
            return Ok(());
        };
        let mut watch_request_sender = self.watch_request_sender.sender_clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(5)).await;
            info!(target: "flows", info);
            let _ = watch_request_sender.send(request).await;
        });

        Ok(())
    }

    async fn on_input_eos(&mut self, flow_path: &Utf8Path) -> Result<(), RuntimeError> {
        if let Some(flow) = self.processor.registry.flow(flow_path) {
            if let Some(request) = flow.watch_request() {
                info!(target: "flows", "Reconnecting input: {flow_path}: {}", flow.as_ref().input);
                self.watch_request_sender.send(request).await?
            };
        }
        Ok(())
    }

    async fn publish_result(&mut self, result: FlowResult) -> Result<(), RuntimeError> {
        match result {
            FlowResult::Ok {
                flow,
                messages,
                output,
            } => self.publish(&flow, messages, &output).await,
            FlowResult::Err {
                flow,
                error,
                output,
            } => self.publish_error(&flow, error, &output).await,
        }
    }

    async fn publish(
        &mut self,
        flow: &Utf8Path,
        messages: Vec<Message>,
        output: &FlowOutput,
    ) -> Result<(), RuntimeError> {
        match output {
            FlowOutput::Mqtt { topic } => {
                for mut message in messages {
                    if let Some(output_topic) = topic {
                        message.topic = output_topic.name.clone();
                    }
                    match MqttMessage::try_from(message) {
                        Ok(message) => self.mqtt_sender.send(message).await?,
                        Err(err) => {
                            error!(target: "flows", "{flow}: cannot publish transformed message: {err}")
                        }
                    }
                }
            }
            FlowOutput::File { path } => {
                let Ok(file) = tokio::fs::File::options()
                    .create(true)
                    .append(true)
                    .open(path)
                    .await
                    .map_err(|err| {
                        error!(target: "flows", "{flow}: cannot open {path}: {err}");
                    })
                else {
                    return Ok(());
                };
                let mut file = tokio::io::BufWriter::new(file);
                for message in messages {
                    if let Err(err) = file.write_all(format!("{message}\n").as_bytes()).await {
                        error!(target: "flows", "{flow}: cannot append to {path}: {err}");
                    }
                }
                if let Err(err) = file.flush().await {
                    error!(target: "flows", "{flow}: cannot flush {path}: {err}");
                }
            }
            FlowOutput::Context => {
                // Any context output reaching this point is an error
                for message in messages {
                    error!(target: "flows", "{flow}: cannot store context value: {}", message.payload_str().unwrap_or_default());
                }
            }
        }
        Ok(())
    }

    async fn publish_error(
        &mut self,
        flow: &Utf8Path,
        error: FlowError,
        output: &FlowOutput,
    ) -> Result<(), RuntimeError> {
        let message = Message::new("", format!("Error in {flow}: {error}"));
        self.publish(flow, vec![message], output).await
    }

    async fn handle_fs_event(&mut self, event: FsWatchEvent) -> Result<(), RuntimeError> {
        match event {
            FsWatchEvent::DirectoryCreated(path) | FsWatchEvent::Modified(path) => {
                // note: when a directory is moved into the watched directory, it triggers Modified in the project's dev
                // container, but (only) DirectoryCreated in debian-systemd test image
                let Ok(path) = Utf8PathBuf::try_from(path) else {
                    return Ok(());
                };
                self.on_path_updated(path.as_path()).await?;
            }
            FsWatchEvent::FileDeleted(path) | FsWatchEvent::DirectoryDeleted(path) => {
                let Ok(path) = Utf8PathBuf::try_from(path) else {
                    return Ok(());
                };
                self.on_path_removed(path.as_path()).await?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Update/remove files as required after a path was modified.
    async fn on_path_updated(&mut self, path: &Utf8Path) -> Result<(), RuntimeError> {
        if path.is_dir() {
            // we can get Modified with path to a directory, which means another directory
            // was moved into flows dir.
            let Ok(entries) = std::fs::read_dir(path).inspect_err(
                |error| error!(%path, ?error, "Failed to read inside flows directory"),
            ) else {
                return Ok(());
            };
            for entry in entries {
                let Ok(entry) = entry.inspect_err(
                    |error| error!(%path, ?error, "Failed to read inside flows directory"),
                ) else {
                    continue;
                };
                let Ok(path) = Utf8PathBuf::try_from(entry.path()) else {
                    error!(?path, "Invalid path");
                    continue;
                };

                self.on_file_updated(path.as_path()).await?;
            }
        } else if path.is_file() {
            self.on_file_updated(path).await?;
        } else if !path.exists() {
            self.on_path_removed(path).await?;
        }

        Ok(())
    }

    /// Remove all flows and scripts that are currently loaded if they are prefixed by the path.
    async fn on_path_removed(&mut self, path: &Utf8Path) -> Result<(), RuntimeError> {
        let (removed_flows, removed_scripts): (Vec<_>, Vec<_>) = self
            .loaded_files
            .iter()
            .filter(|p| p.starts_with(path))
            .cloned()
            // remove flows before scripts, otherwise a warning is printed
            .partition(|p| p.extension() == Some("toml"));
        for file in removed_flows {
            self.on_file_removed(&file).await?;
        }
        for file in removed_scripts {
            self.on_file_removed(&file).await?;
        }

        Ok(())
    }

    async fn on_file_updated(&mut self, path: &Utf8Path) -> Result<(), RuntimeError> {
        if matches!(path.extension(), Some("js" | "ts" | "mjs")) {
            let reloaded_flows = self.processor.reload_script(path).await;
            self.send_updated_subscriptions().await?;
            self.update_all_flow_status(reloaded_flows).await?;
            self.loaded_files.insert(path.into());
        } else if path.extension() == Some("toml") {
            self.processor.add_flow(path).await;
            self.send_updated_subscriptions().await?;
            self.update_flow_status(path).await?;
            self.loaded_files.insert(path.into());
        }
        Ok(())
    }

    async fn on_file_removed(&mut self, path: &Utf8Path) -> Result<(), RuntimeError> {
        if matches!(path.extension(), Some("js" | "ts" | "mjs")) {
            self.processor.remove_script(path).await;
            self.loaded_files.remove(path);
        } else if path.extension() == Some("toml") {
            self.processor.remove_flow(path).await;
            self.send_updated_subscriptions().await?;
            self.update_flow_status(path).await?;
            self.loaded_files.remove(path);
        }
        Ok(())
    }
}
