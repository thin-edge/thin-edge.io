use crate::flow::DateTime;
use crate::flow::Message;
use async_trait::async_trait;
use camino::Utf8PathBuf;
use std::time::Duration;
use tokio::time::Instant;

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

pub struct CommandFlowInput {
    topic: String,
    command: String,
    poll: PollInterval,
}

impl CommandFlowInput {
    pub fn new(topic: String, command: String, interval: Option<Duration>) -> Self {
        CommandFlowInput {
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

pub struct FileFlowInput {
    topic: String,
    path: Utf8PathBuf,
    poll: PollInterval,
}

impl FileFlowInput {
    pub fn new(topic: String, path: Utf8PathBuf, interval: Option<Duration>) -> Self {
        FileFlowInput {
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
