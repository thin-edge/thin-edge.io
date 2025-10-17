use crate::database::DatabaseError;
use crate::database::MeaDb;
use crate::flow::DateTime;
use crate::flow::Message;
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::Instant;

/// Trait for input sources that can be polled for messages
#[async_trait]
pub trait InputSource: Send + Sync {
    /// Poll the source for any available messages at the given timestamp
    /// Returns messages with their timestamps
    async fn poll(
        &mut self,
        timestamp: DateTime,
    ) -> Result<Vec<(DateTime, Message)>, InputSourceError>;

    /// Get the next deadline when this source should be polled
    /// Returns None if the source doesn't have scheduled polling
    fn next_deadline(&self) -> Option<Instant>;

    /// Check if this source is ready to be polled at the current time
    fn is_ready(&self, now: DateTime) -> bool;

    /// Update internal state after a poll (e.g., reschedule next deadline)
    fn update_after_poll(&mut self, now: DateTime);
}

#[derive(thiserror::Error, Debug)]
pub enum InputSourceError {
    #[error("Database error: {0}")]
    Database(#[from] DatabaseError),

    #[error("Source error: {0}")]
    Other(String),
}

/// Input source that drains messages from a specific database series
pub struct DatabaseSource {
    database: Arc<Mutex<Box<dyn MeaDb>>>,
    series: String,
    frequency: Duration,
    max_age: Duration,
    next_drain: Option<Instant>,
    last_drain: Option<DateTime>,
}

impl DatabaseSource {
    pub fn new(
        database: Arc<Mutex<Box<dyn MeaDb>>>,
        series: String,
        frequency: Duration,
        max_age: Duration,
    ) -> Self {
        let next_drain = if !frequency.is_zero() {
            Some(Instant::now() + frequency)
        } else {
            None
        };

        Self {
            database,
            series,
            frequency,
            max_age,
            next_drain,
            last_drain: None,
        }
    }

    fn should_drain_now(&self, timestamp: DateTime) -> bool {
        if self.frequency.is_zero() {
            return false;
        }

        match self.last_drain {
            Some(last_drain) => {
                let elapsed_secs = timestamp.seconds.saturating_sub(last_drain.seconds);
                elapsed_secs >= self.frequency.as_secs()
            }
            None => true, // First drain
        }
    }
}

#[async_trait]
impl InputSource for DatabaseSource {
    async fn poll(
        &mut self,
        timestamp: DateTime,
    ) -> Result<Vec<(DateTime, Message)>, InputSourceError> {
        if !self.should_drain_now(timestamp) {
            return Ok(vec![]);
        }

        let cutoff_time = timestamp - self.max_age;
        let mut db = self.database.lock().await;
        let messages = db.drain_older_than(cutoff_time, &self.series).await?;

        Ok(messages)
    }

    fn next_deadline(&self) -> Option<Instant> {
        self.next_drain
    }

    fn is_ready(&self, now: DateTime) -> bool {
        self.should_drain_now(now)
    }

    fn update_after_poll(&mut self, now: DateTime) {
        self.last_drain = Some(now);

        if !self.frequency.is_zero() {
            self.next_drain = Some(Instant::now() + self.frequency);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::InMemoryMeaDb;
    use std::sync::Arc;
    use time::macros::datetime;
    use tokio::sync::Mutex;

    #[tokio::test]
    async fn database_source_drains_messages() {
        let db = Box::new(InMemoryMeaDb::default()) as Box<dyn MeaDb>;
        let db = Arc::new(Mutex::new(db));
        let series = "test_series";

        let timestamp = DateTime::try_from(datetime!(2023-01-01 10:00 UTC)).unwrap();
        let message = Message::new("test/topic", b"test payload");

        {
            let mut db_lock = db.lock().await;
            db_lock
                .store(series, timestamp, message.clone())
                .await
                .unwrap();
        }

        let mut source = DatabaseSource::new(
            db.clone(),
            series.to_string(),
            Duration::from_secs(60),
            Duration::from_secs(3600),
        );

        let poll_time = DateTime::try_from(datetime!(2023-01-01 11:00 UTC)).unwrap();

        // First poll should drain the message
        let messages = source.poll(poll_time).await.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].0, timestamp);
    }

    #[tokio::test]
    async fn database_source_respects_frequency() {
        let db = Box::new(InMemoryMeaDb::default()) as Box<dyn MeaDb>;
        let db = Arc::new(Mutex::new(db));

        let mut source = DatabaseSource::new(
            db,
            "test".to_string(),
            Duration::from_secs(60),
            Duration::from_secs(3600),
        );

        let poll_time = DateTime::now();

        // First poll
        let _ = source.poll(poll_time).await.unwrap();
        source.update_after_poll(poll_time);

        // Immediate second poll should return nothing
        let messages = source.poll(poll_time).await.unwrap();
        assert_eq!(messages.len(), 0);
    }

    #[tokio::test]
    async fn database_source_next_deadline_updates() {
        let db = Box::new(InMemoryMeaDb::default()) as Box<dyn MeaDb>;
        let db = Arc::new(Mutex::new(db));

        let mut source = DatabaseSource::new(
            db,
            "test".to_string(),
            Duration::from_secs(60),
            Duration::from_secs(3600),
        );

        assert!(source.next_deadline().is_some());

        source.update_after_poll(DateTime::now());
        assert!(source.next_deadline().is_some());
    }
}
