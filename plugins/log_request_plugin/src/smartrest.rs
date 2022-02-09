use c8y_smartrest::smartrest_deserializer::{get_datetime_from_file_path, SmartRestLogRequest};
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
        .filter_map(|dir_entry| {
            let file_path = &dir_entry.path();
            let datetime_object = get_datetime_from_file_path(&file_path);
            match datetime_object {
                Ok(dt) => {
                    if dt < smartrest_obj.date_from || dt > smartrest_obj.date_to {
                        return None;
                    }
                    Some(dir_entry)
                }
                Err(_) => None,
            }
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
