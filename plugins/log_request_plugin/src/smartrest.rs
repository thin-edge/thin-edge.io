use c8y_smartrest::{
    smartrest_deserializer::{get_datetime_from_file_path, SmartRestLogRequest},
    smartrest_serializer::{
        CumulocitySupportedOperations, SmartRestSerializer, SmartRestSetOperationToExecuting,
        SmartRestSetOperationToSuccessful,
    },
    topic::C8yTopic,
};
use mqtt_channel::Message;

/// returns a c8y message specifying to set log status to executing.
///
/// example message: '501,c8y_LogfileRequest'
pub async fn get_log_file_request_executing() -> Result<Message, anyhow::Error> {
    let topic = C8yTopic::SmartRestResponse.to_topic()?;
    let smartrest_set_operation_status =
        SmartRestSetOperationToExecuting::new(CumulocitySupportedOperations::C8yLogFileRequest)
            .to_smartrest()?;
    Ok(Message::new(&topic, smartrest_set_operation_status))
}

/// returns a c8y message specifying to set log status to successful.
///
/// example message: '503,c8y_LogfileRequest,https://{c8y.url}/etc...'
pub async fn get_log_file_request_done_message(
    binary_upload_event_url: &str,
) -> Result<Message, anyhow::Error> {
    let topic = C8yTopic::SmartRestResponse.to_topic()?;
    let smartrest_set_operation_status =
        SmartRestSetOperationToSuccessful::new(CumulocitySupportedOperations::C8yLogFileRequest)
            .with_response_parameter(binary_upload_event_url)
            .to_smartrest()?;

    Ok(Message::new(&topic, smartrest_set_operation_status))
}

/// Reads tedge logs according to `SmartRestLogRequest`.
///
/// If needed, logs are concatenated.
///
/// Logs are sorted alphanumerically from oldest to newest.
///
/// # Examples
///
/// ```
/// let smartrest_obj = SmartRestLogRequest::from_smartrest(
///     "522,DeviceSerial,syslog,2021-01-01T00:00:00+0200,2021-01-10T00:00:00+0200,,1000",
/// )
/// .unwrap();
///
/// let log = read_tedge_system_logs(&smartrest_obj, "/var/log/tedge").unwrap();
/// ```
pub fn read_tedge_logs(
    smartrest_obj: &SmartRestLogRequest,
    logs_dir: &str,
) -> Result<String, anyhow::Error> {
    let mut output = String::new();

    // NOTE: As per documentation of std::fs::read_dir:
    // "The order in which this iterator returns entries is platform and filesystem dependent."
    // Therefore, files are sorted by date.
    let mut read_vector: Vec<_> = std::fs::read_dir(logs_dir)?
        .filter_map(|r| r.ok())
        .filter(|dir_entry| {
            get_datetime_from_file_path(&dir_entry.path())
                .map(|dt| !(dt < smartrest_obj.date_from || dt > smartrest_obj.date_to))
                .unwrap_or(false)
        })
        .filter(|dir_entry| {
            let file_name = &dir_entry.file_name();
            let file_name = file_name.to_str().unwrap();
            file_name.starts_with(&smartrest_obj.log_type)
        })
        .collect();

    read_vector.sort_by_key(|dir| dir.path());

    // loop sorted vector and push store log file to `output`
    let mut line_counter: usize = 0;
    for entry in read_vector {
        let file_path = entry.path();
        let file_content = std::fs::read_to_string(&file_path)?;
        if file_content.is_empty() {
            continue;
        }

        // adding file header only if line_counter permits more lines to be added
        match &file_path.file_stem().and_then(|f| f.to_str()) {
            Some(file_name) if line_counter < smartrest_obj.lines => {
                output.push_str(&format!("filename: {}\n", file_name));
            }
            _ => {}
        }

        // split at new line delimiter ("\n")
        let mut lines = file_content.lines();
        while line_counter < smartrest_obj.lines {
            if let Some(haystack) = lines.next() {
                if let Some(needle) = &smartrest_obj.needle {
                    if haystack.contains(needle) {
                        output.push_str(&format!("{}\n", haystack));
                        line_counter += 1;
                    }
                } else {
                    output.push_str(&format!("{}\n", haystack));
                    line_counter += 1;
                }
            } else {
                // there are no lines.next()
                break;
            }
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::read_tedge_logs;
    use c8y_smartrest::smartrest_deserializer::SmartRestLogRequest;
    use std::fs::File;
    use std::io::Write;

    fn parse_file_names_from_log_content(log_content: &str) -> [&str; 5] {
        let mut files: Vec<&str> = vec![];
        for line in log_content.lines() {
            if line.contains("filename: ") {
                let filename: &str = line.split("filename: ").last().unwrap();
                files.push(filename);
            }
        }
        match files.try_into() {
            Ok(arr) => arr,
            Err(_) => panic!("Could not convert to Array &str, size 5"),
        }
    }

    #[test]
    /// testing read_tedge_logs
    ///
    /// this test creates 5 fake log files in a temporary directory.
    /// files are dated 2021-01-0XT01:00Z, where X = a different day.
    ///
    /// this tests will assert that files are read alphanumerically from oldest to newest
    fn test_read_logs() {
        // order in which files are created
        const LOG_FILE_NAMES: [&str; 5] = [
            "software-list-2021-01-03T01:00:00Z.log",
            "software-list-2021-01-02T01:00:00Z.log",
            "software-list-2021-01-01T01:00:00Z.log",
            "software-update-2021-01-03T01:00:00Z.log",
            "software-update-2021-01-02T01:00:00Z.log",
        ];

        // expected (sorted) output
        const EXPECTED_OUTPUT: [&str; 5] = [
            "software-list-2021-01-01T01:00:00Z",
            "software-list-2021-01-02T01:00:00Z",
            "software-list-2021-01-03T01:00:00Z",
            "software-update-2021-01-02T01:00:00Z",
            "software-update-2021-01-03T01:00:00Z",
        ];

        let smartrest_obj = SmartRestLogRequest::from_smartrest(
            "522,DeviceSerial,software,2021-01-01T00:00:00+0200,2021-01-10T00:00:00+0200,,1000",
        )
        .unwrap();

        let temp_dir = tempfile::tempdir().unwrap();
        // creating the files
        for (idx, file) in LOG_FILE_NAMES.iter().enumerate() {
            let file_path = &temp_dir.path().join(file);
            let mut file = File::create(file_path).unwrap();
            writeln!(file, "file num {}", idx).unwrap();
        }

        // reading the logs and extracting the file names from the log output.
        let output = read_tedge_logs(&smartrest_obj, temp_dir.path().to_str().unwrap()).unwrap();
        let parsed_values = parse_file_names_from_log_content(&output);

        // asserting the order = `EXPECTED_OUTPUT`
        assert!(parsed_values.eq(&EXPECTED_OUTPUT));
    }
}
