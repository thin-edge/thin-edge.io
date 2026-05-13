//! The message log is a persistent append-only log of MQTT messages.
//! Each line is the JSON representation of that MQTT message.
//! The underlying file is a JSON lines file.
use mqtt_channel::MqttMessage;
use serde_json::json;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::BufRead;
use std::io::BufReader;
use std::io::BufWriter;
use std::io::Write;
use std::path::Path;

const LOG_FILE_NAME: &str = "entity_store.jsonl";
const LOG_FORMAT_VERSION: &str = "1.0";

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
}

impl MessageLogWriter {
    pub fn new<P>(log_dir: P) -> Result<MessageLogWriter, std::io::Error>
    where
        P: AsRef<Path>,
    {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_dir.as_ref().join(LOG_FILE_NAME))?;

        // If the file is empty append the version information as a header
        let metadata = file.metadata()?;
        let file_is_empty = metadata.len() == 0;

        let mut writer = BufWriter::new(file);

        if file_is_empty {
            let version_info = json!({ "version": LOG_FORMAT_VERSION }).to_string();
            writeln!(writer, "{}", version_info)?;
        }

        Ok(MessageLogWriter { writer })
    }

    pub fn new_truncated<P>(log_dir: P) -> Result<MessageLogWriter, std::io::Error>
    where
        P: AsRef<Path>,
    {
        let _ = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(log_dir.as_ref().join(LOG_FILE_NAME))?;

        MessageLogWriter::new(log_dir)
    }

    /// Append the JSON representation of the given message to the log.
    /// Each message is appended on a new line.
    pub fn append_message(&mut self, message: &MqttMessage) -> Result<(), std::io::Error> {
        let json_line = serde_json::to_string(message)?;
        writeln!(self.writer, "{}", json_line)?;
        self.writer.flush()?;
        self.writer.get_ref().sync_all()?;
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
        writer.append_message(&make_message("topic", "payload")).unwrap();

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

    fn make_message(topic: &str, payload: &str) -> MqttMessage {
        MqttMessage::new(&Topic::new(topic).unwrap(), payload)
    }
}
