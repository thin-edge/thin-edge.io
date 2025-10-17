use crate::flow::DateTime;
use crate::flow::Message;
use crate::flow::SourceTag;
use async_trait::async_trait;
use camino::Utf8PathBuf;
use std::fmt::Display;
use std::time::Duration;
use tedge_mqtt_ext::TopicFilter;
use tedge_watch_ext::WatchRequest;
use tokio::time::Instant;

pub trait FlowInput: Display + StreamingSource + PollingSource {}
impl<T: Display + StreamingSource + PollingSource> FlowInput for T {}

/// Trait for input sources that stream messages out of continuously running processes
pub trait StreamingSource {
    /// MQTT topics subscribed by this source
    fn topics(&self) -> TopicFilter {
        TopicFilter::empty()
    }

    /// Topic to be used when messages are not received from MQTT
    fn enforced_topic(&self) -> Option<&str> {
        None
    }

    /// Process watched by this source
    fn watch_request(&self) -> Option<WatchRequest> {
        None
    }

    fn accept_message(&self, source: &SourceTag, message: &Message) -> bool;
}

/// Trait for input sources that can be polled for messages
#[async_trait]
pub trait PollingSource: Send + Sync {
    /// Poll the source for any available messages at the given timestamp
    async fn poll(&mut self, timestamp: DateTime) -> Result<Vec<Message>, PollingSourceError>;

    /// Get the next deadline when this source should be polled
    /// Returns None if the source doesn't have scheduled polling
    fn next_deadline(&self) -> Option<Instant>;

    /// Check if this source is ready to be polled at the current time
    fn is_ready(&self, now: Instant) -> bool;

    /// Update internal state after a poll (e.g., reschedule next deadline)
    fn update_after_poll(&mut self, now: Instant);
}

#[derive(thiserror::Error, Debug)]
pub enum PollingSourceError {
    #[error("Fail to poll {resource}: {error}")]
    CannotPoll { resource: String, error: String },
}

pub struct MqttFlowInput {
    pub topics: TopicFilter,
}

impl Display for MqttFlowInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MQTT topics: {:?}", self.topics)
    }
}

#[async_trait]
impl PollingSource for MqttFlowInput {
    async fn poll(&mut self, _timestamp: DateTime) -> Result<Vec<Message>, PollingSourceError> {
        Ok(vec![])
    }

    fn next_deadline(&self) -> Option<Instant> {
        None
    }

    fn is_ready(&self, _now: Instant) -> bool {
        false
    }

    fn update_after_poll(&mut self, _now: Instant) {}
}

impl StreamingSource for MqttFlowInput {
    fn topics(&self) -> TopicFilter {
        self.topics.clone()
    }

    fn accept_message(&self, source: &SourceTag, message: &Message) -> bool {
        match source {
            SourceTag::Mqtt => self.topics.accept_topic_name(&message.topic),
            _ => false,
        }
    }
}

pub struct CommandFlowInput {
    flow: String,
    topic: String,
    command: String,
    poll: PollInterval,
}

impl Display for CommandFlowInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Command output: {}", self.command)
    }
}

impl CommandFlowInput {
    pub fn new(flow: String, topic: String, command: String, interval: Option<Duration>) -> Self {
        CommandFlowInput {
            flow,
            topic,
            command,
            poll: PollInterval::new(interval),
        }
    }
}

/// A CommandFlowInput works as a polling source when a polling interval as been set.
#[async_trait]
impl PollingSource for CommandFlowInput {
    async fn poll(&mut self, timestamp: DateTime) -> Result<Vec<Message>, PollingSourceError> {
        let output = tedge_watch_ext::command_output(&self.command)
            .await
            .map_err(|err| PollingSourceError::CannotPoll {
                resource: self.command.clone(),
                error: err.to_string(),
            })?;
        let messages = output
            .lines()
            .map(|payload| Message::with_timestamp(self.topic.clone(), payload, timestamp))
            .collect();

        Ok(messages)
    }

    fn next_deadline(&self) -> Option<Instant> {
        self.poll.next_deadline()
    }

    fn is_ready(&self, now: Instant) -> bool {
        self.poll.is_ready(now)
    }

    fn update_after_poll(&mut self, now: Instant) {
        self.poll.update_after_poll(now);
    }
}

impl StreamingSource for CommandFlowInput {
    fn enforced_topic(&self) -> Option<&str> {
        Some(&self.topic)
    }

    fn watch_request(&self) -> Option<WatchRequest> {
        if self.poll.is_polling() {
            None
        } else {
            Some(WatchRequest::WatchCommand {
                topic: self.flow.clone(),
                command: self.command.clone(),
            })
        }
    }

    fn accept_message(&self, source: &SourceTag, _message: &Message) -> bool {
        match source {
            SourceTag::Mqtt => false,
            SourceTag::Process { .. } if self.poll.is_polling() => false,
            SourceTag::Process { flow } => flow == &self.flow,
            SourceTag::Poll { flow } if self.poll.is_polling() => flow == &self.flow,
            SourceTag::Poll { .. } => false,
        }
    }
}

pub struct FileFlowInput {
    flow: String,
    topic: String,
    path: Utf8PathBuf,
    poll: PollInterval,
}

impl Display for FileFlowInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "File content: {}", self.path)
    }
}

impl FileFlowInput {
    pub fn new(flow: String, topic: String, path: Utf8PathBuf, interval: Option<Duration>) -> Self {
        FileFlowInput {
            flow,
            topic,
            path,
            poll: PollInterval::new(interval),
        }
    }
}

#[async_trait]
impl PollingSource for FileFlowInput {
    async fn poll(&mut self, timestamp: DateTime) -> Result<Vec<Message>, PollingSourceError> {
        let output = tokio::fs::read_to_string(&self.path).await.map_err(|err| {
            PollingSourceError::CannotPoll {
                resource: self.path.clone().to_string(),
                error: err.to_string(),
            }
        })?;
        let messages = output
            .lines()
            .map(|payload| Message::with_timestamp(self.topic.clone(), payload, timestamp))
            .collect();
        Ok(messages)
    }

    fn next_deadline(&self) -> Option<Instant> {
        self.poll.next_deadline()
    }

    fn is_ready(&self, now: Instant) -> bool {
        self.poll.is_ready(now)
    }

    fn update_after_poll(&mut self, now: Instant) {
        self.poll.update_after_poll(now);
    }
}

impl StreamingSource for FileFlowInput {
    fn enforced_topic(&self) -> Option<&str> {
        Some(&self.topic)
    }

    fn watch_request(&self) -> Option<WatchRequest> {
        if self.poll.is_polling() {
            None
        } else {
            Some(WatchRequest::WatchFile {
                topic: self.flow.clone(),
                file: self.path.clone(),
            })
        }
    }

    fn accept_message(&self, source: &SourceTag, _message: &Message) -> bool {
        match source {
            SourceTag::Mqtt => false,
            SourceTag::Process { .. } if self.poll.is_polling() => false,
            SourceTag::Process { flow } => flow == &self.flow,
            SourceTag::Poll { flow } if self.poll.is_polling() => flow == &self.flow,
            SourceTag::Poll { .. } => false,
        }
    }
}

struct PollInterval {
    polling_interval: Option<Duration>,
    next_deadline: Option<Instant>,
}

impl PollInterval {
    fn new(interval: Option<Duration>) -> Self {
        let polling_interval = match interval {
            Some(interval) if !interval.is_zero() => Some(interval),
            _ => None,
        };
        PollInterval {
            polling_interval,
            next_deadline: None,
        }
    }

    fn is_polling(&self) -> bool {
        self.polling_interval.is_some()
    }

    fn next_deadline(&self) -> Option<Instant> {
        self.polling_interval?;
        self.next_deadline
    }

    fn is_ready(&self, now: Instant) -> bool {
        self.polling_interval.is_some()
            && (self.next_deadline.is_none() || self.next_deadline.unwrap() < now)
    }

    fn update_after_poll(&mut self, now: Instant) {
        if let Some(interval) = self.polling_interval {
            self.next_deadline = Some(now + interval);
        }
    }
}
