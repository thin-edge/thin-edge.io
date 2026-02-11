use crate::flow::Message;
use async_trait::async_trait;
use camino::Utf8PathBuf;
use std::time::Duration;
use std::time::SystemTime;
use tedge_watch_ext::WatchRequest;
use tokio::time::Instant;

/// Trait for input sources that stream messages out of continuously running processes
pub trait StreamingSource: Send + Sync {
    /// Process watched by this source
    fn watch_request(&self) -> Option<WatchRequest>;
}

/// Trait for input sources that can be polled for messages
#[async_trait]
pub trait PollingSource: Send + Sync {
    /// Poll the source for any available messages at the given timestamp
    async fn poll(&mut self, timestamp: SystemTime) -> Result<Vec<Message>, PollingSourceError>;

    /// Get the next deadline when this source should be polled
    fn next_deadline(&self) -> Instant;

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

pub struct CommandPollingSource {
    topic: String,
    command: String,
    poll: PollInterval,
}

impl CommandPollingSource {
    pub fn new(topic: String, command: String, interval: Duration) -> Self {
        CommandPollingSource {
            topic,
            command,
            poll: PollInterval::new(interval),
        }
    }
}

#[async_trait]
impl PollingSource for CommandPollingSource {
    async fn poll(&mut self, timestamp: SystemTime) -> Result<Vec<Message>, PollingSourceError> {
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

    fn next_deadline(&self) -> Instant {
        self.poll.next_deadline
    }

    fn is_ready(&self, now: Instant) -> bool {
        self.poll.is_ready(now)
    }

    fn update_after_poll(&mut self, now: Instant) {
        self.poll.update_after_poll(now);
    }
}

pub struct CommandStreamingSource {
    flow: String,
    command: String,
}

impl CommandStreamingSource {
    pub fn new(flow: String, command: String) -> Self {
        CommandStreamingSource { flow, command }
    }
}

impl StreamingSource for CommandStreamingSource {
    fn watch_request(&self) -> Option<WatchRequest> {
        Some(WatchRequest::WatchCommand {
            topic: self.flow.clone(),
            command: self.command.clone(),
        })
    }
}

pub struct FilePollingSource {
    topic: String,
    path: Utf8PathBuf,
    poll: PollInterval,
}

impl FilePollingSource {
    pub fn new(topic: String, path: Utf8PathBuf, interval: Duration) -> Self {
        FilePollingSource {
            topic,
            path,
            poll: PollInterval::new(interval),
        }
    }
}

#[async_trait]
impl PollingSource for FilePollingSource {
    async fn poll(&mut self, timestamp: SystemTime) -> Result<Vec<Message>, PollingSourceError> {
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

    fn next_deadline(&self) -> Instant {
        self.poll.next_deadline
    }

    fn is_ready(&self, now: Instant) -> bool {
        self.poll.is_ready(now)
    }

    fn update_after_poll(&mut self, now: Instant) {
        self.poll.update_after_poll(now);
    }
}

pub struct FileStreamingSource {
    flow: String,
    path: Utf8PathBuf,
}

impl FileStreamingSource {
    pub fn new(flow: String, path: Utf8PathBuf) -> Self {
        FileStreamingSource { flow, path }
    }
}

impl StreamingSource for FileStreamingSource {
    fn watch_request(&self) -> Option<WatchRequest> {
        Some(WatchRequest::WatchFile {
            topic: self.flow.clone(),
            file: self.path.clone(),
        })
    }
}

struct PollInterval {
    polling_interval: Duration,
    next_deadline: Instant,
}

impl PollInterval {
    fn new(polling_interval: Duration) -> Self {
        assert!(
            !polling_interval.is_zero(),
            "A polling interval must be non-zero"
        );
        PollInterval {
            polling_interval,
            // Use a small delay before the first interval to
            // reduce noise when the poller is instantiated in quick succession
            // but don't wait for entire interval before the first deadline
            next_deadline: Instant::now() + Duration::from_secs(2),
        }
    }

    fn is_ready(&self, now: Instant) -> bool {
        self.next_deadline <= now
    }

    fn update_after_poll(&mut self, now: Instant) {
        self.next_deadline = now + self.polling_interval;
    }
}
