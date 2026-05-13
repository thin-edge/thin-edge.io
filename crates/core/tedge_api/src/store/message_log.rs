//! The message log is a persistent append-only log of MQTT messages.
//! Each line is the JSON representation of that MQTT message.
//! The underlying file is a JSON lines file.
use mqtt_channel::MqttMessage;
use serde_json::json;
use std::collections::HashSet;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::BufRead;
use std::io::BufReader;
use std::io::BufWriter;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

const LOG_FILE_NAME: &str = "entity_store.jsonl";
const LOG_FILE_TEMP_NAME: &str = "entity_store.jsonl.tmp";
const LOG_FORMAT_VERSION: &str = "1.0";
const DEFAULT_REDUNDANCY_THRESHOLD: usize = 100;

#[derive(thiserror::Error, Debug)]
pub enum LogEntryError {
    #[error(transparent)]
    FromStdIo(std::io::Error),

    #[error("Deserialization failed with {0} while parsing {1}")]
    FromSerdeJson(#[source] serde_json::Error, String),
}

/// A reader to read the log file entries line by line
pub(crate) struct MessageLogReader {
    reader: BufReader<File>,
}

impl MessageLogReader {
    pub fn new<P>(log_dir: P) -> Result<MessageLogReader, std::io::Error>
    where
        P: AsRef<Path>,
    {
        let file = OpenOptions::new()
            .read(true)
            .open(log_dir.as_ref().join(LOG_FILE_NAME))?;
        let mut reader = BufReader::new(file);

        let mut version_info = String::new();
        reader.read_line(&mut version_info)?;
        // TODO: Validate if the read version is supported

        Ok(MessageLogReader { reader })
    }

    /// Return the next MQTT message from the log
    /// The reads start from the beginning of the file
    /// and each read advances the file pointer to the next line
    pub fn next_message(&mut self) -> Result<Option<MqttMessage>, LogEntryError> {
        let mut buffer = String::new();
        match self.reader.read_line(&mut buffer) {
            Ok(bytes_read) if bytes_read > 0 => {
                let message: MqttMessage = serde_json::from_str(&buffer)
                    .map_err(|err| LogEntryError::FromSerdeJson(err, buffer))?;
                Ok(Some(message))
            }
            Ok(_) => Ok(None), // EOF
            Err(err) => Err(LogEntryError::FromStdIo(err)),
        }
    }
}

/// A writer to append new MQTT messages to the end of the log
pub(crate) struct MessageLogWriter {
    writer: BufWriter<File>,
    log_dir: PathBuf,
    redundancy_threshold: usize,
    unique_topics: HashSet<String>,
    total_entries: usize,
}

impl MessageLogWriter {
    pub fn new<P>(log_dir: P) -> Result<MessageLogWriter, std::io::Error>
    where
        P: AsRef<Path>,
    {
        Self::open(log_dir, DEFAULT_REDUNDANCY_THRESHOLD, false)
    }

    pub fn new_truncated<P>(log_dir: P) -> Result<MessageLogWriter, std::io::Error>
    where
        P: AsRef<Path>,
    {
        Self::open(log_dir, DEFAULT_REDUNDANCY_THRESHOLD, true)
    }

    #[cfg(test)]
    pub fn new_with_redundancy_threshold<P>(
        log_dir: P,
        redundancy_threshold: usize,
    ) -> Result<MessageLogWriter, std::io::Error>
    where
        P: AsRef<Path>,
    {
        Self::open(log_dir, redundancy_threshold, false)
    }

    fn open<P>(
        log_dir: P,
        redundancy_threshold: usize,
        truncate: bool,
    ) -> Result<MessageLogWriter, std::io::Error>
    where
        P: AsRef<Path>,
    {
        let log_dir = log_dir.as_ref();

        if truncate {
            OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(log_dir.join(LOG_FILE_NAME))?;
        }

        // Read the existing file to build the topic index for accurate redundancy tracking.
        let mut unique_topics = HashSet::new();
        let mut total_entries = 0;
        if let Ok(mut reader) = MessageLogReader::new(log_dir) {
            while let Some(msg) = reader.next_message().map_err(std::io::Error::other)? {
                unique_topics.insert(msg.topic.name.clone());
                total_entries += 1;
            }
        }

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_dir.join(LOG_FILE_NAME))?;

        let metadata = file.metadata()?;
        let mut writer = BufWriter::new(file);

        if metadata.len() == 0 {
            writeln!(writer, "{}", json!({ "version": LOG_FORMAT_VERSION }))?;
        }

        Ok(MessageLogWriter {
            writer,
            log_dir: log_dir.to_path_buf(),
            redundancy_threshold,
            unique_topics,
            total_entries,
        })
    }

    /// Append the JSON representation of the given message to the log.
    /// Each message is appended on a new line.
    pub fn append_message(&mut self, message: &MqttMessage) -> Result<(), std::io::Error> {
        let json_line = serde_json::to_string(message)?;
        writeln!(self.writer, "{}", json_line)?;
        self.writer.flush()?;
        self.writer.get_ref().sync_all()?;

        let is_new_topic = self.unique_topics.insert(message.topic.name.clone());
        self.total_entries += 1;

        if !is_new_topic {
            let redundant_count = self.total_entries - self.unique_topics.len();
            if redundant_count >= self.redundancy_threshold {
                self.compact()?;
            }
        }

        Ok(())
    }

    fn compact(&mut self) -> Result<(), std::io::Error> {
        self.writer.flush()?;

        let mut all_messages = Vec::new();
        let mut reader = MessageLogReader::new(&self.log_dir)?;
        while let Some(msg) = reader.next_message().map_err(std::io::Error::other)? {
            all_messages.push(msg);
        }
        drop(reader);

        // Keep the last occurrence of each topic, in order of last write (move-to-end semantics).
        let mut seen = HashSet::new();
        let mut deduped: Vec<_> = all_messages
            .into_iter()
            .rev()
            .filter(|msg| seen.insert(msg.topic.name.clone()))
            .collect();
        deduped.reverse();

        // Write to a temp file so that any failure leaves the original log intact.
        let temp_path = self.log_dir.join(LOG_FILE_TEMP_NAME);
        {
            let temp_file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&temp_path)?;
            let mut temp_writer = BufWriter::new(temp_file);

            writeln!(temp_writer, "{}", json!({ "version": LOG_FORMAT_VERSION }))?;
            for msg in &deduped {
                writeln!(temp_writer, "{}", serde_json::to_string(msg)?)?;
            }
            temp_writer.flush()?;
            temp_writer.get_ref().sync_all()?;
        } // temp_file closed before rename

        // Atomically replace the log with the compacted version.
        std::fs::rename(&temp_path, self.log_dir.join(LOG_FILE_NAME))?;

        let file = OpenOptions::new()
            .append(true)
            .open(self.log_dir.join(LOG_FILE_NAME))?;
        self.writer = BufWriter::new(file);
        self.total_entries = deduped.len();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::MessageLogReader;
    use super::MessageLogWriter;
    use mqtt_channel::MqttMessage;
    use mqtt_channel::Topic;
    use tempfile::tempdir;

    #[test]
    fn reading_from_empty_log_returns_none() {
        let temp_dir = tempdir().unwrap();
        MessageLogWriter::new(&temp_dir).unwrap();
        let mut reader = MessageLogReader::new(&temp_dir).unwrap();
        assert_eq!(reader.next_message().unwrap(), None);
    }

    #[test]
    fn messages_are_read_back_in_the_order_they_were_written() {
        let temp_dir = tempdir().unwrap();
        let messages = [
            make_message("topic1", "payload1"),
            make_message("topic2", "payload2"),
            make_message("topic3", "payload3"),
        ];

        let mut writer = MessageLogWriter::new(&temp_dir).unwrap();
        for message in &messages {
            writer.append_message(message).unwrap();
        }

        let mut reader = MessageLogReader::new(&temp_dir).unwrap();
        for expected in &messages {
            assert_eq!(reader.next_message().unwrap().as_ref(), Some(expected));
        }
    }

    #[test]
    fn reading_past_the_last_message_returns_none() {
        let temp_dir = tempdir().unwrap();

        let mut writer = MessageLogWriter::new(&temp_dir).unwrap();
        writer
            .append_message(&make_message("topic", "payload"))
            .unwrap();

        let mut reader = MessageLogReader::new(&temp_dir).unwrap();
        reader.next_message().unwrap();
        assert_eq!(reader.next_message().unwrap(), None);
    }

    #[test]
    fn truncated_log_discards_previously_written_messages() {
        let temp_dir = tempdir().unwrap();

        let mut writer = MessageLogWriter::new(&temp_dir).unwrap();
        for i in 1..=3 {
            writer
                .append_message(&make_message(&format!("topic{i}"), &format!("payload{i}")))
                .unwrap();
        }

        MessageLogWriter::new_truncated(&temp_dir).unwrap();

        let mut reader = MessageLogReader::new(&temp_dir).unwrap();
        assert_eq!(reader.next_message().unwrap(), None);
    }

    #[test]
    fn compaction_deduplicates_topics_when_redundancy_threshold_is_exceeded() {
        let temp_dir = tempdir().unwrap();

        // threshold=1: compact as soon as there is 1 redundant entry
        let mut writer = MessageLogWriter::new_with_redundancy_threshold(&temp_dir, 1).unwrap();
        writer
            .append_message(&make_message("topic1", "v1"))
            .unwrap(); // redundant=0
        writer
            .append_message(&make_message("topic2", "v2"))
            .unwrap(); // redundant=0
        writer
            .append_message(&make_message("topic1", "v3"))
            .unwrap(); // redundant=1 → compact

        // After compaction: one entry per topic, latest value, move-to-end order
        let mut reader = MessageLogReader::new(&temp_dir).unwrap();
        assert_eq!(
            reader.next_message().unwrap(),
            Some(make_message("topic2", "v2"))
        );
        assert_eq!(
            reader.next_message().unwrap(),
            Some(make_message("topic1", "v3"))
        );
        assert_eq!(reader.next_message().unwrap(), None);
    }

    #[test]
    fn writing_to_new_topics_does_not_trigger_compaction() {
        let temp_dir = tempdir().unwrap();

        // threshold=1, but all topics are unique — redundant count stays 0, no compaction
        let mut writer = MessageLogWriter::new_with_redundancy_threshold(&temp_dir, 1).unwrap();
        let messages: Vec<_> = (0..5)
            .map(|i| make_message(&format!("topic{i}"), "v1"))
            .collect();
        for msg in &messages {
            writer.append_message(msg).unwrap();
        }

        // All 5 messages should be present — no compaction triggered
        let mut reader = MessageLogReader::new(&temp_dir).unwrap();
        for expected in &messages {
            assert_eq!(reader.next_message().unwrap().as_ref(), Some(expected));
        }
        assert_eq!(reader.next_message().unwrap(), None);
    }

    #[test]
    fn duplicates_below_the_threshold_are_not_compacted() {
        let temp_dir = tempdir().unwrap();

        // threshold=3: tolerates up to 2 redundant entries; compacts only at 3
        let mut writer = MessageLogWriter::new_with_redundancy_threshold(&temp_dir, 3).unwrap();
        writer
            .append_message(&make_message("topic1", "v1"))
            .unwrap(); // redundant=0
        writer
            .append_message(&make_message("topic1", "v2"))
            .unwrap(); // redundant=1
        writer
            .append_message(&make_message("topic1", "v3"))
            .unwrap(); // redundant=2, still below 3

        let mut reader = MessageLogReader::new(&temp_dir).unwrap();
        assert_eq!(
            reader.next_message().unwrap(),
            Some(make_message("topic1", "v1"))
        );
        assert_eq!(
            reader.next_message().unwrap(),
            Some(make_message("topic1", "v2"))
        );
        assert_eq!(
            reader.next_message().unwrap(),
            Some(make_message("topic1", "v3"))
        );
        assert_eq!(reader.next_message().unwrap(), None);
    }

    #[test]
    fn writer_can_continue_writing_after_compaction() {
        let temp_dir = tempdir().unwrap();

        let mut writer = MessageLogWriter::new_with_redundancy_threshold(&temp_dir, 1).unwrap();
        writer
            .append_message(&make_message("topic1", "v1"))
            .unwrap();
        writer
            .append_message(&make_message("topic1", "v2"))
            .unwrap(); // redundant=1 → compact

        // Write more messages after compaction
        writer
            .append_message(&make_message("topic2", "v1"))
            .unwrap();
        writer
            .append_message(&make_message("topic2", "v2"))
            .unwrap(); // redundant=1 → compact again

        let mut reader = MessageLogReader::new(&temp_dir).unwrap();
        assert_eq!(
            reader.next_message().unwrap(),
            Some(make_message("topic1", "v2"))
        );
        assert_eq!(
            reader.next_message().unwrap(),
            Some(make_message("topic2", "v2"))
        );
        assert_eq!(reader.next_message().unwrap(), None);
    }

    #[test]
    fn new_writer_correctly_loads_compacted_state() {
        let temp_dir = tempdir().unwrap();

        // First writer: trigger compaction, then drop
        {
            let mut writer = MessageLogWriter::new_with_redundancy_threshold(&temp_dir, 1).unwrap();
            writer
                .append_message(&make_message("topic1", "v1"))
                .unwrap();
            writer
                .append_message(&make_message("topic2", "v2"))
                .unwrap();
            writer
                .append_message(&make_message("topic1", "v3"))
                .unwrap(); // redundant=1 → compact
        }

        // Second writer: reads compacted state (2 unique topics, 0 redundant)
        // One more duplicate should immediately trigger another compaction.
        let mut writer = MessageLogWriter::new_with_redundancy_threshold(&temp_dir, 1).unwrap();
        writer
            .append_message(&make_message("topic2", "v4"))
            .unwrap(); // redundant=1 → compact

        let mut reader = MessageLogReader::new(&temp_dir).unwrap();
        assert_eq!(
            reader.next_message().unwrap(),
            Some(make_message("topic1", "v3"))
        );
        assert_eq!(
            reader.next_message().unwrap(),
            Some(make_message("topic2", "v4"))
        );
        assert_eq!(reader.next_message().unwrap(), None);
    }

    fn make_message(topic: &str, payload: &str) -> MqttMessage {
        MqttMessage::new(&Topic::new(topic).unwrap(), payload)
    }
}
