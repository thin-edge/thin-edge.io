//! Database abstraction for time-series message storage

use crate::flow::DateTime;
use crate::flow::Message;
use anyhow::Context;
use async_trait::async_trait;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use fjall::Keyspace;
use fjall::PartitionCreateOptions;
use fjall::Slice;
use thiserror::Error;
use tokio::task::spawn_blocking;

/// Errors that can occur during database operations
#[derive(Error, Debug)]
pub enum DatabaseError {
    #[error("Failed to open database at {path:?}: {source}")]
    OpenError {
        path: Utf8PathBuf,
        #[source]
        source: anyhow::Error,
    },

    #[error("Failed to store message in series {series:?}: {source}")]
    StoreError {
        series: String,
        #[source]
        source: anyhow::Error,
    },

    #[error("Failed to drain messages from series {series:?}: {source}")]
    DrainError {
        series: String,
        #[source]
        source: anyhow::Error,
    },

    #[error("Failed to query series {series:?}: {source}")]
    QueryError {
        series: String,
        #[source]
        source: anyhow::Error,
    },

    #[error("Database operation failed: {source}")]
    Internal {
        #[source]
        source: anyhow::Error,
    },
}

impl From<fjall::Error> for DatabaseError {
    fn from(err: fjall::Error) -> Self {
        DatabaseError::Internal {
            source: anyhow::Error::from(err),
        }
    }
}

/// Trait for database operations on time-series data
#[async_trait]
pub trait MeaDb: Send + Sync {
    /// Store a message in the specified series at the given timestamp
    async fn store(
        &mut self,
        series: &str,
        timestamp: DateTime,
        payload: Message,
    ) -> Result<(), DatabaseError>;

    /// Drain all messages older than or equal to the cutoff timestamp from the specified series
    ///
    /// This operation removes the messages from the database and returns them.
    /// Messages are returned in chronological order.
    async fn drain_older_than(
        &mut self,
        cutoff: DateTime,
        series: &str,
    ) -> Result<Vec<(DateTime, Message)>, DatabaseError>;

    /// Query all messages in the specified series
    ///
    /// Messages are returned in chronological order.
    async fn query_all(&mut self, series: &str) -> Result<Vec<(DateTime, Message)>, DatabaseError>;
}

/// Database service implementation using fjall as the storage backend
pub struct FjallMeaDb {
    keyspace: Keyspace,
}

impl FjallMeaDb {
    /// Open a database at the specified path
    pub async fn open(path: impl AsRef<Utf8Path>) -> Result<Self, DatabaseError> {
        let path = path.as_ref().to_owned();
        let config = fjall::Config::new(&path);

        let keyspace = spawn_blocking(move || config.open())
            .await
            .expect("database open task should not panic")
            .with_context(|| format!("opening database at {path:?}"))
            .map_err(|source| DatabaseError::OpenError { path, source })?;

        Ok(Self { keyspace })
    }
}

#[async_trait]
impl MeaDb for FjallMeaDb {
    async fn store(
        &mut self,
        series: &str,
        timestamp: DateTime,
        payload: Message,
    ) -> Result<(), DatabaseError> {
        let ks = self.keyspace.clone();
        let series_owned = series.to_owned();
        let series_for_error = series.to_owned();

        spawn_blocking(move || {
            let partition = ks.open_partition(&series_owned, PartitionCreateOptions::default())?;
            partition.insert(timestamp.to_slice(), payload.to_slice())?;
            Ok(())
        })
        .await
        .expect("database store task should not panic")
        .map_err(|e: fjall::Error| DatabaseError::StoreError {
            series: series_for_error,
            source: anyhow::Error::from(e),
        })
    }

    async fn drain_older_than(
        &mut self,
        cutoff: DateTime,
        series: &str,
    ) -> Result<Vec<(DateTime, Message)>, DatabaseError> {
        let ks = self.keyspace.clone();
        let series_owned = series.to_owned();
        let series_for_error = series.to_owned();

        spawn_blocking(move || {
            let partition = ks.open_partition(&series_owned, PartitionCreateOptions::default())?;
            let messages = partition
                .range(..=cutoff.to_slice())
                .map(|res| res.map(decode_message))
                .collect::<Result<Vec<_>, _>>()?;

            // Remove the messages after collecting them
            for (timestamp, _) in &messages {
                partition.remove(timestamp.to_slice())?;
            }

            Ok(messages)
        })
        .await
        .expect("database drain task should not panic")
        .map_err(|e: fjall::Error| DatabaseError::DrainError {
            series: series_for_error,
            source: anyhow::Error::from(e),
        })
    }

    async fn query_all(&mut self, series: &str) -> Result<Vec<(DateTime, Message)>, DatabaseError> {
        let ks = self.keyspace.clone();
        let series_owned = series.to_owned();
        let series_for_error = series.to_owned();

        spawn_blocking(move || {
            let partition = ks.open_partition(&series_owned, PartitionCreateOptions::default())?;
            let messages = partition
                .iter()
                .map(|res| res.map(decode_message))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(messages)
        })
        .await
        .expect("database query task should not panic")
        .map_err(|e: fjall::Error| DatabaseError::QueryError {
            series: series_for_error,
            source: anyhow::Error::from(e),
        })
    }
}

/// Helper trait for converting types to/from fjall Slice format
pub trait ToFromSlice {
    fn to_slice(&self) -> Slice;
    fn from_slice(slice: Slice) -> Self;
}

impl ToFromSlice for DateTime {
    fn to_slice(&self) -> Slice {
        let mut arr = [0u8; 12];
        arr[0..8].copy_from_slice(&self.seconds.to_be_bytes());
        arr[8..12].copy_from_slice(&self.nanoseconds.to_be_bytes());
        Slice::new(&arr)
    }

    fn from_slice(slice: Slice) -> Self {
        let secs_be = &slice[..8];
        let nanos_be = &slice[8..];
        let secs = u64::from_be_bytes(secs_be.try_into().unwrap());
        let nanos = u32::from_be_bytes(nanos_be.try_into().unwrap());

        Self {
            seconds: secs,
            nanoseconds: nanos,
        }
    }
}

impl ToFromSlice for Message {
    fn to_slice(&self) -> Slice {
        Slice::new(self.json().to_string().as_bytes())
    }

    fn from_slice(slice: Slice) -> Self {
        serde_json::from_slice(&slice).unwrap()
    }
}

fn decode_message((key, value): (Slice, Slice)) -> (DateTime, Message) {
    (DateTime::from_slice(key), Message::from_slice(value))
}

/// In-memory database implementation for testing
#[cfg(test)]
#[derive(Default)]
pub struct InMemoryMeaDb {
    data: std::collections::HashMap<String, std::collections::BTreeMap<DateTime, Message>>,
}

#[cfg(test)]
#[async_trait]
impl MeaDb for InMemoryMeaDb {
    async fn store(
        &mut self,
        series: &str,
        timestamp: DateTime,
        payload: Message,
    ) -> Result<(), DatabaseError> {
        self.data
            .entry(series.to_owned())
            .or_default()
            .insert(timestamp, payload);
        Ok(())
    }

    async fn drain_older_than(
        &mut self,
        cutoff: DateTime,
        series: &str,
    ) -> Result<Vec<(DateTime, Message)>, DatabaseError> {
        let series_data = self.data.entry(series.to_owned()).or_default();

        // Find all entries <= cutoff
        let mut drained = Vec::new();
        let mut keys_to_remove = Vec::new();

        for (&timestamp, message) in series_data.iter() {
            if timestamp <= cutoff {
                drained.push((timestamp, message.clone()));
                keys_to_remove.push(timestamp);
            }
        }

        // Remove the drained entries
        for key in keys_to_remove {
            series_data.remove(&key);
        }

        Ok(drained)
    }

    async fn query_all(&mut self, series: &str) -> Result<Vec<(DateTime, Message)>, DatabaseError> {
        let series_data = self.data.entry(series.to_owned()).or_default();
        Ok(series_data
            .iter()
            .map(|(&timestamp, message)| (timestamp, message.clone()))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::DateTime;
    use crate::flow::Message;
    use camino::Utf8PathBuf;
    use futures::future::BoxFuture;
    use rstest::rstest;
    use time::macros::datetime;

    type DbFactory = fn() -> BoxFuture<'static, Box<dyn MeaDb>>;

    #[rstest]
    #[case::fjall(create_fjall_db)]
    #[case::inmemory(create_inmemory_db)]
    #[tokio::test]
    async fn stored_message_can_be_retrieved(#[case] db_factory: DbFactory) {
        let mut db = db_factory().await;

        let series = "sensor_data";
        let seconds = datetime!(2023-01-01 10:00 UTC).unix_timestamp();
        let timestamp = DateTime {
            seconds: seconds as u64,
            nanoseconds: 0,
        };
        let message = test_message("test/topic", "temp: 25C");

        db.store(series, timestamp, message.clone())
            .await
            .expect("store should succeed");

        // Verify the message was stored
        let stored_messages = db.drain_older_than(timestamp, series).await.unwrap();
        assert_eq!(stored_messages, [(timestamp, message)]);
    }

    #[rstest]
    #[case::fjall(create_fjall_db)]
    #[case::inmemory(create_inmemory_db)]
    #[tokio::test]
    async fn stored_messages_are_retrieved_in_chronological_order(#[case] db_factory: DbFactory) {
        let mut db = db_factory().await;

        let series = "sensor_data";
        let ts1 = datetime!(2023-01-01 10:00 UTC).unix_timestamp();
        let ts1 = DateTime {
            seconds: ts1 as u64,
            nanoseconds: 0,
        };
        let msg1 = test_message("test/topic1", "temp: 25C");
        let ts2 = datetime!(2023-01-01 10:05 UTC).unix_timestamp();
        let ts2 = DateTime {
            seconds: ts2 as u64,
            nanoseconds: 0,
        };
        let msg2 = test_message("test/topic2", "temp: 26C");
        let ts3 = datetime!(2023-01-01 09:55 UTC).unix_timestamp();
        let ts3 = DateTime {
            seconds: ts3 as u64,
            nanoseconds: 0,
        };
        let msg3 = test_message("test/topic3", "temp: 24C");

        db.store(series, ts1, msg1.clone()).await.unwrap();
        db.store(series, ts2, msg2.clone()).await.unwrap();
        db.store(series, ts3, msg3.clone()).await.unwrap();

        let stored_messages = db.drain_older_than(ts2, series).await.unwrap();

        // Verify messages are sorted by timestamp
        assert_eq!(stored_messages, [(ts3, msg3), (ts1, msg1), (ts2, msg2)]);
    }

    #[rstest]
    #[case::fjall(create_fjall_db)]
    #[case::inmemory(create_inmemory_db)]
    #[tokio::test]
    async fn messages_in_different_series_remain_isolated(#[case] db_factory: DbFactory) {
        let mut db = db_factory().await;

        let series1 = "sensor_data_a";
        let ts1 = datetime!(2023-01-01 10:00 UTC).unix_timestamp();
        let ts1 = DateTime {
            seconds: ts1 as u64,
            nanoseconds: 0,
        };
        let msg1 = test_message("test/topic1", "data A1");

        let series2 = "sensor_data_b";
        let ts2 = datetime!(2023-01-01 10:01 UTC).unix_timestamp();
        let ts2 = DateTime {
            seconds: ts2 as u64,
            nanoseconds: 0,
        };
        let msg2 = test_message("test/topic2", "data B1");

        db.store(series1, ts1, msg1.clone()).await.unwrap();
        db.store(series2, ts2, msg2.clone()).await.unwrap();

        let s1_data = db.drain_older_than(ts1, series1).await.unwrap();
        let s2_data = db.drain_older_than(ts2, series2).await.unwrap();
        assert_eq!(s1_data, [(ts1, msg1)]);
        assert_eq!(s2_data, [(ts2, msg2)]);
    }

    #[rstest]
    #[case::fjall(create_fjall_db)]
    #[case::inmemory(create_inmemory_db)]
    #[tokio::test]
    async fn drained_messages_are_removed_from_database(#[case] db_factory: DbFactory) {
        let mut db = db_factory().await;

        let series = "sensor_data_a";
        let timestamp = datetime!(2023-01-01 10:00 UTC).unix_timestamp();
        let timestamp = DateTime {
            seconds: timestamp as u64,
            nanoseconds: 0,
        };
        let msg = test_message("test/topic", "data A1");

        db.store(series, timestamp, msg.clone()).await.unwrap();

        let data = db.drain_older_than(timestamp, series).await.unwrap();
        assert_eq!(data, [(timestamp, msg.clone())]);
        let data_after_drain = db.drain_older_than(timestamp, series).await.unwrap();
        assert_eq!(data_after_drain, []);
    }

    #[rstest]
    #[case::fjall(create_fjall_db)]
    #[case::inmemory(create_inmemory_db)]
    #[tokio::test]
    async fn queried_messages_are_returned_in_chronological_order(#[case] db_factory: DbFactory) {
        let mut db = db_factory().await;

        let series = "test_series";
        let ts1 = datetime!(2023-01-01 10:00 UTC).unix_timestamp();
        let ts1 = DateTime {
            seconds: ts1 as u64,
            nanoseconds: 0,
        };
        let msg1 = test_message("test/topic1", "message 1");

        let ts2 = datetime!(2023-01-01 10:01 UTC).unix_timestamp();
        let ts2 = DateTime {
            seconds: ts2 as u64,
            nanoseconds: 0,
        };
        let msg2 = test_message("test/topic2", "message 2");

        // Store messages
        db.store(series, ts1, msg1.clone()).await.unwrap();
        db.store(series, ts2, msg2.clone()).await.unwrap();

        // Query all messages
        let all_messages = db.query_all(series).await.unwrap();

        // Messages should be in chronological order
        assert_eq!(all_messages, [(ts1, msg1), (ts2, msg2)]);
    }

    // Database factory functions for rstest
    fn create_fjall_db() -> BoxFuture<'static, Box<dyn MeaDb>> {
        Box::pin(async {
            let temp_dir = tempfile::tempdir().unwrap();
            let path = Utf8PathBuf::from_path_buf(temp_dir.path().join("test_db")).unwrap();
            let db = FjallMeaDb::open(&path).await.unwrap();
            // Keep temp_dir alive by leaking it - this is acceptable for tests
            std::mem::forget(temp_dir);
            Box::new(db) as Box<dyn MeaDb>
        })
    }

    fn create_inmemory_db() -> BoxFuture<'static, Box<dyn MeaDb>> {
        Box::pin(async { Box::new(InMemoryMeaDb::default()) as Box<dyn MeaDb> })
    }

    fn test_message(topic: &str, payload: &str) -> Message {
        Message {
            topic: topic.to_string(),
            payload: payload.into(),
            timestamp: Some(DateTime::now()),
        }
    }
}
