//! The message log is a persistent append-only log of MQTT messages.
//! Each line is the JSON representation of that MQTT message.
//! The underlying file is a JSON lines file.
use crate::mqtt_topics::is_entity_twin_topic;
use indexmap::IndexMap;
use mqtt_channel::MqttMessage;
use mqtt_channel::Topic;
use serde_json::json;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::BufRead;
use std::io::BufReader;
use std::io::BufWriter;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use tracing::warn;

const LOG_FILE_NAME: &str = "entity_store.jsonl";
const LOG_FILE_TEMP_NAME: &str = "entity_store.jsonl.tmp";
const LOG_FORMAT_VERSION: &str = "1.0";
const DEFAULT_REDUNDANCY_THRESHOLD: usize = 100;

/// A persistent append-only log of MQTT messages.
///
/// Tracks the latest payload per topic and compacts the on-disk file when
/// redundant entries exceed a configured threshold.
pub(crate) struct MessageLog {
    writer: BufWriter<File>,
    log_dir: PathBuf,
    redundancy_threshold: usize,
    // Latest payload per topic; topics removed by empty-payload writes are absent
    messages: IndexMap<String, String>,
    // Number of entries on disk; subtracting messages.len() gives the redundant entry count
    total_entries: usize,
}

impl MessageLog {
    pub fn new<P>(log_dir: P) -> Result<MessageLog, std::io::Error>
    where
        P: AsRef<Path>,
    {
        Self::open(log_dir, DEFAULT_REDUNDANCY_THRESHOLD, false)
    }

    pub fn new_truncated<P>(log_dir: P) -> Result<MessageLog, std::io::Error>
    where
        P: AsRef<Path>,
    {
        Self::open(log_dir, DEFAULT_REDUNDANCY_THRESHOLD, true)
    }

    #[cfg(test)]
    pub fn new_with_redundancy_threshold<P>(
        log_dir: P,
        redundancy_threshold: usize,
    ) -> Result<MessageLog, std::io::Error>
    where
        P: AsRef<Path>,
    {
        Self::open(log_dir, redundancy_threshold, false)
    }

    fn open<P>(
        log_dir: P,
        redundancy_threshold: usize,
        truncate: bool,
    ) -> Result<MessageLog, std::io::Error>
    where
        P: AsRef<Path>,
    {
        let log_dir = log_dir.as_ref();
        let log_path = log_dir.join(LOG_FILE_NAME);

        let mut messages = IndexMap::new();
        let mut total_entries = 0;

        let file = if !truncate {
            if let Ok(entries) = Self::read_file(log_dir) {
                for (topic, payload) in entries {
                    total_entries += 1;
                    if payload.is_empty() {
                        messages.shift_remove(&topic);
                    } else {
                        messages.insert(topic, payload);
                    }
                }
            }

            OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)?
        } else {
            OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&log_path)?
        };

        let metadata = file.metadata()?;
        let mut writer = BufWriter::new(file);

        if metadata.len() == 0 {
            writeln!(writer, "{}", json!({ "version": LOG_FORMAT_VERSION }))?;
        }

        Ok(MessageLog {
            writer,
            log_dir: log_dir.to_path_buf(),
            redundancy_threshold,
            messages,
            total_entries,
        })
    }

    /// Reads raw (topic, payload) pairs from the log file on disk
    fn read_file(log_dir: &Path) -> Result<Vec<(String, String)>, std::io::Error> {
        let file = OpenOptions::new()
            .read(true)
            .open(log_dir.join(LOG_FILE_NAME))?;
        let mut reader = BufReader::new(file);

        // Skip the version line
        let mut version_info = String::new();
        reader.read_line(&mut version_info)?;

        let mut entries = Vec::new();
        let mut buffer = String::new();
        loop {
            buffer.clear();
            match reader.read_line(&mut buffer) {
                Ok(0) => break,
                Ok(_) => match serde_json::from_str::<MqttMessage>(&buffer) {
                    Ok(message) => {
                        let topic = message.topic.name.clone();
                        if is_entity_twin_topic(&topic) {
                            continue;
                        }
                        let payload = String::from_utf8_lossy(message.payload_bytes()).into_owned();
                        entries.push((topic, payload));
                    }
                    Err(e) => warn!("Skipping corrupt entity store log entry: {e}"),
                },
                Err(err) => return Err(err),
            }
        }
        Ok(entries)
    }

    /// Iterates over the latest message per topic, in the order topics were first seen
    pub fn messages(&self) -> impl Iterator<Item = MqttMessage> + '_ {
        self.messages.iter().map(|(topic, payload)| {
            MqttMessage::new(&Topic::new_unchecked(topic), payload.as_str())
        })
    }

    /// Persists the message to the log
    pub fn append_message(&mut self, message: &MqttMessage) -> Result<(), std::io::Error> {
        if is_entity_twin_topic(&message.topic.name) {
            return Ok(());
        }

        let json_line = serde_json::to_string(message)?;
        writeln!(self.writer, "{}", json_line)?;
        self.writer.flush()?;
        self.writer.get_ref().sync_all()?;

        let topic = message.topic.name.clone();
        let payload = String::from_utf8_lossy(message.payload_bytes()).into_owned();

        let is_new_topic = !self.messages.contains_key(&topic);
        if payload.is_empty() {
            self.messages.shift_remove(&topic);
        } else {
            self.messages.insert(topic, payload);
        }
        self.total_entries += 1;

        if !is_new_topic {
            let redundant_count = self.total_entries - self.messages.len();
            if redundant_count >= self.redundancy_threshold {
                self.compact()?;
            }
        }

        Ok(())
    }

    fn compact(&mut self) -> Result<(), std::io::Error> {
        // Write the in-memory compacted state to a temp file.
        let temp_path = self.log_dir.join(LOG_FILE_TEMP_NAME);
        {
            let temp_file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&temp_path)?;
            let mut temp_writer = BufWriter::new(temp_file);

            writeln!(temp_writer, "{}", json!({ "version": LOG_FORMAT_VERSION }))?;
            for (topic, payload) in &self.messages {
                let msg = MqttMessage::new(&Topic::new_unchecked(topic), payload.as_str());
                writeln!(temp_writer, "{}", serde_json::to_string(&msg)?)?;
            }
            temp_writer.flush()?;
            temp_writer.get_ref().sync_all()?;
        }

        // Atomically replace the log with the compacted version.
        std::fs::rename(&temp_path, self.log_dir.join(LOG_FILE_NAME))?;

        let file = OpenOptions::new()
            .append(true)
            .open(self.log_dir.join(LOG_FILE_NAME))?;
        self.writer = BufWriter::new(file);
        self.total_entries = self.messages.len();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::MessageLog;
    use mqtt_channel::MqttMessage;
    use mqtt_channel::Topic;
    use serde_json::json;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn reading_from_empty_log_returns_none() {
        let log = MessageLog::new(tempdir().unwrap()).unwrap();
        assert_eq!(log.messages().next(), None);
    }

    #[test]
    fn messages_are_read_back_in_the_order_they_were_written() {
        let temp_dir = tempdir().unwrap();
        let messages = [
            make_message("topic1", "payload1"),
            make_message("topic2", "payload2"),
            make_message("topic3", "payload3"),
        ];

        let mut log = MessageLog::new(&temp_dir).unwrap();
        for message in &messages {
            log.append_message(message).unwrap();
        }

        let read_messages: Vec<_> = log.messages().collect();
        assert_eq!(read_messages, messages);
    }

    #[test]
    fn truncated_log_discards_previously_written_messages() {
        let temp_dir = tempdir().unwrap();

        let mut log = MessageLog::new(&temp_dir).unwrap();
        for i in 1..=3 {
            log.append_message(&make_message(&format!("topic{i}"), &format!("payload{i}")))
                .unwrap();
        }

        let log = MessageLog::new_truncated(&temp_dir).unwrap();
        assert_eq!(log.messages().next(), None);
    }

    #[test]
    fn truncated_log_resets_redundancy_tracking() {
        let temp_dir = tempdir().unwrap();

        let mut log = MessageLog::new(&temp_dir).unwrap();
        log.append_message(&make_message("topic", "payload"))
            .unwrap();

        // After truncation, redundancy tracking must start from zero: a single
        // duplicate write at threshold=1 should immediately trigger compaction.
        let mut log = MessageLog::new_with_redundancy_threshold(&temp_dir, 1).unwrap();
        log.append_message(&make_message("topic", "v1")).unwrap();
        log.append_message(&make_message("topic", "v2")).unwrap(); // redundant=1 → compact
        assert_eq!(
            log.messages().collect::<Vec<_>>(),
            vec![make_message("topic", "v2")]
        );
    }

    #[test]
    fn messages_are_persisted_across_log_instances() {
        let temp_dir = tempdir().unwrap();

        {
            let mut log = MessageLog::new(&temp_dir).unwrap();
            log.append_message(&make_message("topic1", "v1")).unwrap();
            log.append_message(&make_message("topic2", "v2")).unwrap();
            log.append_message(&make_message("topic3", "v3")).unwrap();
        }

        let log = MessageLog::new(&temp_dir).unwrap();
        assert_eq!(
            log.messages().collect::<Vec<_>>(),
            vec![
                make_message("topic1", "v1"),
                make_message("topic2", "v2"),
                make_message("topic3", "v3"),
            ]
        );
    }

    #[test]
    fn compaction_keeps_parent_before_child_in_first_surviving_topic_order() {
        let temp_dir = tempdir().unwrap();

        // threshold=1: compact as soon as there is 1 redundant entry
        let mut log = MessageLog::new_with_redundancy_threshold(&temp_dir, 1).unwrap();
        log.append_message(&make_message(
            "te/device/child0//",
            r#"{"@type":"child-device"}"#,
        ))
        .unwrap();
        log.append_message(&make_message(
            "te/device/child01//",
            r#"{"@type":"child-device","@parent":"device/child0"}"#,
        ))
        .unwrap();
        log.append_message(&make_message(
            "te/device/child0//",
            r#"{"@type":"child-device","name":"Child 0"}"#,
        ))
        .unwrap(); // redundant=1 → compact

        // After compaction: latest value per topic, keeping parent before child for replay.
        let read_messages: Vec<_> = log.messages().collect();
        assert_eq!(
            read_messages,
            vec![
                make_message(
                    "te/device/child0//",
                    r#"{"@type":"child-device","name":"Child 0"}"#
                ),
                make_message(
                    "te/device/child01//",
                    r#"{"@type":"child-device","@parent":"device/child0"}"#
                ),
            ]
        );
    }

    #[test]
    fn compaction_removes_topics_where_latest_message_is_empty() {
        let temp_dir = tempdir().unwrap();

        let mut log = MessageLog::new_with_redundancy_threshold(&temp_dir, 1).unwrap();
        log.append_message(&make_message("topic1", "v1")).unwrap();
        log.append_message(&make_message("topic2", "v2")).unwrap();
        log.append_message(&make_message("topic1", "")).unwrap();

        let read_messages: Vec<_> = log.messages().collect();
        assert_eq!(read_messages, vec![make_message("topic2", "v2")]);
    }

    #[test]
    fn compaction_removes_twin_messages_from_existing_log_files() {
        let temp_dir = tempdir().unwrap();
        let log_path = temp_dir.path().join("entity_store.jsonl");
        let mut file = std::fs::File::create(&log_path).unwrap();
        let entity_v1 = make_message(
            "te/device/child//",
            r#"{"@type":"child-device","name":"v1"}"#,
        );
        let twin = make_message("te/device/child///twin/name", r#""legacy-twin""#);
        writeln!(file, "{}", json!({ "version": "1.0" })).unwrap();
        writeln!(file, "{}", serde_json::to_string(&entity_v1).unwrap()).unwrap();
        writeln!(file, "{}", serde_json::to_string(&twin).unwrap()).unwrap();

        let mut log = MessageLog::new_with_redundancy_threshold(&temp_dir, 1).unwrap();
        log.append_message(&make_message(
            "te/device/child//",
            r#"{"@type":"child-device","name":"v2"}"#,
        ))
        .unwrap();

        let compacted_log = std::fs::read_to_string(log_path).unwrap();
        assert!(compacted_log.contains("te/device/child//"));
        assert!(compacted_log.contains(r#""name\":\"v2"#));
        assert!(!compacted_log.contains("te/device/child///twin/name"));
        assert!(!compacted_log.contains("legacy-twin"));
    }

    #[test]
    fn compaction_does_not_preserve_position_after_empty_payload() {
        let temp_dir = tempdir().unwrap();

        // threshold=1: compact as soon as there is 1 redundant entry
        let mut log = MessageLog::new_with_redundancy_threshold(&temp_dir, 1).unwrap();
        log.append_message(&make_message(
            "te/device/child_a//",
            r#"{"@type":"child-device"}"#,
        ))
        .unwrap();
        log.append_message(&make_message(
            "te/device/child_b//",
            r#"{"@type":"child-device"}"#,
        ))
        .unwrap();
        log.append_message(&make_message("te/device/child_a//", ""))
            .unwrap(); // redundant=1 → compact; child_a removed

        // child_a is no longer in messages after compaction, so this is treated as a new topic
        log.append_message(&make_message(
            "te/device/child_a//",
            r#"{"@type":"child-device","name":"Child A"}"#,
        ))
        .unwrap();

        // child_b retains its original position; child_a lost its position
        let read_messages: Vec<_> = log.messages().collect();
        assert_eq!(
            read_messages,
            vec![
                make_message("te/device/child_b//", r#"{"@type":"child-device"}"#),
                make_message(
                    "te/device/child_a//",
                    r#"{"@type":"child-device","name":"Child A"}"#
                ),
            ]
        );
    }

    #[test]
    fn writing_to_new_topics_does_not_trigger_compaction() {
        let temp_dir = tempdir().unwrap();

        // threshold=1, but all topics are unique — redundant count stays 0, no compaction
        let mut log = MessageLog::new_with_redundancy_threshold(&temp_dir, 1).unwrap();
        let messages: Vec<_> = (0..5)
            .map(|i| make_message(&format!("topic{i}"), "v1"))
            .collect();
        for msg in &messages {
            log.append_message(msg).unwrap();
        }

        // All 5 messages should be present — no compaction triggered
        let read_messages: Vec<_> = log.messages().collect();
        assert_eq!(read_messages, messages);
    }

    #[test]
    fn duplicates_below_the_threshold_are_not_compacted() {
        let temp_dir = tempdir().unwrap();

        // threshold=3: tolerates up to 2 redundant entries; compacts only at 3
        let mut log = MessageLog::new_with_redundancy_threshold(&temp_dir, 3).unwrap();
        log.append_message(&make_message("topic1", "v1")).unwrap();
        log.append_message(&make_message("topic1", "v2")).unwrap();
        log.append_message(&make_message("topic1", "v3")).unwrap(); // redundant=2, still below 3

        // The in-memory state only keeps the latest, but total_entries tracks disk state
        let read_messages: Vec<_> = log.messages().collect();
        assert_eq!(read_messages, vec![make_message("topic1", "v3")]);
    }

    #[test]
    fn writer_can_continue_writing_after_compaction() {
        let temp_dir = tempdir().unwrap();

        let mut log = MessageLog::new_with_redundancy_threshold(&temp_dir, 1).unwrap();
        log.append_message(&make_message("topic1", "v1")).unwrap();
        log.append_message(&make_message("topic1", "v2")).unwrap(); // redundant=1 → compact

        // Write more messages after compaction
        log.append_message(&make_message("topic2", "v1")).unwrap();
        log.append_message(&make_message("topic2", "v2")).unwrap(); // redundant=1 → compact again

        let read_messages: Vec<_> = log.messages().collect();
        assert_eq!(
            read_messages,
            vec![make_message("topic1", "v2"), make_message("topic2", "v2"),]
        );
    }

    #[test]
    fn new_log_correctly_loads_compacted_state() {
        let temp_dir = tempdir().unwrap();

        // First log: trigger compaction, then drop
        {
            let mut log = MessageLog::new_with_redundancy_threshold(&temp_dir, 1).unwrap();
            log.append_message(&make_message("topic1", "v1")).unwrap();
            log.append_message(&make_message("topic2", "v2")).unwrap();
            log.append_message(&make_message("topic1", "v3")).unwrap(); // redundant=1 → compact
        }

        // Second log: reads compacted state (2 unique topics, 0 redundant)
        // One more duplicate should immediately trigger another compaction.
        let mut log = MessageLog::new_with_redundancy_threshold(&temp_dir, 1).unwrap();
        log.append_message(&make_message("topic2", "v4")).unwrap(); // redundant=1 → compact

        let read_messages: Vec<_> = log.messages().collect();
        assert_eq!(
            read_messages,
            vec![make_message("topic1", "v3"), make_message("topic2", "v4"),]
        );
    }

    #[test]
    fn corrupt_log_line_is_skipped_and_valid_messages_are_preserved() {
        let temp_dir = tempdir().unwrap();

        {
            let mut log = MessageLog::new(&temp_dir).unwrap();
            log.append_message(&make_message("topic1", "v1")).unwrap();
            log.append_message(&make_message("topic2", "v2")).unwrap();
        }

        // All non-corrupt lines should be read successfully.
        {
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(temp_dir.path().join("entity_store.jsonl"))
                .unwrap();
            writeln!(file, "this is not valid json").unwrap();
            let msg3 = make_message("topic3", "v3");
            writeln!(file, "{}", serde_json::to_string(&msg3).unwrap()).unwrap();
        }

        let log = MessageLog::new(&temp_dir).unwrap();
        let messages: Vec<_> = log.messages().collect();
        assert_eq!(
            messages,
            vec![
                make_message("topic1", "v1"),
                make_message("topic2", "v2"),
                make_message("topic3", "v3"),
            ]
        );
    }

    fn make_message(topic: &str, payload: &str) -> MqttMessage {
        MqttMessage::new(&Topic::new(topic).unwrap(), payload)
    }
}
