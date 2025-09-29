use super::config::FileEntry;
use super::error::LogRetrievalError;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use easy_reader::EasyReader;
use glob::glob;
use regex::Regex;
use std::cmp::Reverse;
use std::collections::VecDeque;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use time::OffsetDateTime;

/// read any log file coming from `obj.log.log_type`
pub fn new_read_logs(
    files: &[FileEntry],
    log_type: &str,
    date_from: OffsetDateTime,
    tmp_dir: &Utf8Path,
) -> Result<Utf8PathBuf, LogRetrievalError> {
    //filter logs on type and date
    let logfiles_to_read = filter_logs(files, log_type, date_from)?;

    let temp_path = tmp_dir.join(format!("{log_type}-{}", rand::random::<u128>()));
    let mut temp_file = File::create(&temp_path)?;

    for logfile in logfiles_to_read {
        match read_log_content(logfile.as_path()) {
            Ok(file_content) => {
                temp_file.write_all(file_content.as_bytes())?;
            }
            Err(error) => {
                temp_file.flush()?;
                return Err(error);
            }
        };
    }

    temp_file.flush()?;

    Ok(temp_path)
}

fn read_log_content(logfile: &Path) -> Result<String, LogRetrievalError> {
    let mut file_content = VecDeque::new();
    let file = std::fs::File::open(logfile)?;
    let file_name = format!(
        "filename: {}\n",
        logfile.file_name().unwrap().to_str().unwrap() // never fails because we check file exists
    );
    let reader = EasyReader::new(file);
    match reader {
        Ok(mut reader) => {
            reader.eof();
            while let Some(line) = reader.prev_line()? {
                file_content.push_front(format!("{}\n", line));
            }

            file_content.push_front(file_name);

            let file_content = file_content
                .iter()
                .map(|x| x.to_string())
                .collect::<String>();
            Ok(file_content)
        }
        Err(_err) => Ok(String::new()),
    }
}

fn filter_logs(
    files: &[FileEntry],
    log_type: &str,
    date_from: OffsetDateTime,
) -> Result<Vec<PathBuf>, LogRetrievalError> {
    let mut file_list = filter_logs_by_type(files, log_type)?;
    sort_logs_by_date(&mut file_list);

    let file_list = filter_logs_by_date(file_list, date_from);

    if file_list.is_empty() {
        Err(LogRetrievalError::NoLogsAvailableForType {
            log_type: log_type.to_string(),
        })
    } else {
        Ok(file_list)
    }
}

fn filter_logs_by_type(
    files: &[FileEntry],
    log_type: &str,
) -> Result<Vec<(PathBuf, OffsetDateTime, bool)>, LogRetrievalError> {
    let mut file_list = Vec::new();
    let wildcard_regex = Regex::new(r"^.*\*.*").unwrap();

    let files: Vec<&str> = files
        .iter()
        .filter(|file| file.config_type.eq(log_type))
        .map(|file| file.path.as_str())
        .collect();

    for file in files {
        let paths = glob(file)?;
        for path in paths {
            let entry = path?;
            file_list.push((
                entry.to_owned(),
                get_modification_date(entry.as_path()),
                wildcard_regex.is_match(file),
            ))
        }
    }

    Ok(file_list)
}

fn get_modification_date(path: impl AsRef<Path>) -> OffsetDateTime {
    if let Ok(metadata) = std::fs::metadata(path) {
        if let Ok(file_modified_time) = metadata.modified() {
            return OffsetDateTime::from(file_modified_time);
        }
    };
    // if the file metadata can not be read, we set the file's metadata
    // to UNIX_EPOCH (Jan 1st 1970)
    OffsetDateTime::UNIX_EPOCH
}

fn filter_logs_by_date(
    files: Vec<(PathBuf, OffsetDateTime, bool)>,
    date_from: OffsetDateTime,
) -> Vec<PathBuf> {
    // include log file with static path no matter whether or not it is in date range
    files
        .into_iter()
        .filter(|(_, modification_time, wildcard_match)| {
            !wildcard_match || *modification_time >= date_from
        })
        .map(|(path, _, _)| path)
        .collect()
}

fn sort_logs_by_date(files: &mut [(PathBuf, OffsetDateTime, bool)]) {
    files.sort_by_key(|(_, modification_time, _)| Reverse(*modification_time));
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;
    use crate::config::FileEntry;
    use filetime::set_file_mtime;
    use filetime::FileTime;
    use tedge_test_utils::fs::TempTedgeDir;
    use time::macros::datetime;

    fn prepare() -> (TempTedgeDir, Vec<FileEntry>) {
        let tempdir = TempTedgeDir::new();
        let tempdir_path = tempdir.path().to_str().unwrap();

        tempdir.file("file_a_one");
        tempdir.file("file_b_one");
        tempdir.file("file_c_two");
        tempdir.file("file_d_one");

        set_file_mtime(
            format!("{tempdir_path}/file_a_one"),
            FileTime::from_unix_time(2, 0),
        )
        .unwrap();
        set_file_mtime(
            format!("{tempdir_path}/file_b_one"),
            FileTime::from_unix_time(3, 0),
        )
        .unwrap();
        set_file_mtime(
            format!("{tempdir_path}/file_c_two"),
            FileTime::from_unix_time(11, 0),
        )
        .unwrap();

        let files: Vec<FileEntry> = vec![
            FileEntry {
                path: format!("{tempdir_path}/*_one"),
                config_type: "type_one".to_string(),
            },
            FileEntry {
                path: format!("{tempdir_path}/*_two"),
                config_type: "type_two".to_string(),
            },
        ];

        (tempdir, files)
    }

    #[test]
    /// Out of logs filtered on type = "type_one", that is: { file_a, file_b, file_d }.
    /// Only logs filtered on metadata remain, that is { file_b, file_d }.
    ///
    /// This is because:
    ///
    /// file_a has timestamp: 1970/01/01 00:00:02
    /// file_b has timestamp: 1970/01/01 00:00:03
    /// file_d has timestamp: (current, not modified)
    ///
    /// The order of the output is { file_d, file_b }, because files are sorted from
    /// most recent to oldest
    fn test_filter_logs_path_containing_wildcard_on_type_and_metadata() {
        let (tempdir, files) = prepare();
        let tempdir_path = tempdir.path().to_str().unwrap();

        let logs = filter_logs(&files, "type_one", datetime!(1970-01-01 00:00:03 +00:00)).unwrap();

        assert_eq!(
            logs,
            vec![
                PathBuf::from(format!("{tempdir_path}/file_d_one")),
                PathBuf::from(format!("{tempdir_path}/file_b_one")),
            ]
        )
    }

    #[test]
    /// Out of logs filtered on type = "type_one", that is: { file_a, file_b, file_d }.
    /// logs filtered on metadata are the same, that is { file_a, file_b, file_d }.
    ///
    /// This is because:
    ///
    /// file_a has timestamp: 1970/01/01 00:00:02
    /// file_b has timestamp: 1970/01/01 00:00:03
    /// file_d has timestamp: (current, not modified)
    ///
    /// However, files are provided with static path so we don't take date range into consideration
    ///
    /// The order of the output is { file_d, file_b, file_a }, because files are sorted from
    /// most recent to oldest
    fn test_filter_logs_with_static_path_on_type_and_metadata() {
        let (tempdir, _) = prepare();
        let tempdir_path = tempdir.path().to_str().unwrap();

        // provide vector of files with static paths
        let files: Vec<FileEntry> = vec![
            FileEntry {
                path: format!("{tempdir_path}/file_a_one"),
                config_type: "type_one".to_string(),
            },
            FileEntry {
                path: format!("{tempdir_path}/file_b_one"),
                config_type: "type_one".to_string(),
            },
            FileEntry {
                path: format!("{tempdir_path}/file_c_two"),
                config_type: "type_two".to_string(),
            },
            FileEntry {
                path: format!("{tempdir_path}/file_d_one"),
                config_type: "type_one".to_string(),
            },
        ];

        let logs = filter_logs(&files, "type_one", datetime!(1970-01-01 00:00:03 +00:00)).unwrap();

        assert_eq!(
            logs,
            vec![
                PathBuf::from(format!("{tempdir_path}/file_d_one")),
                PathBuf::from(format!("{tempdir_path}/file_b_one")),
                PathBuf::from(format!("{tempdir_path}/file_a_one")),
            ]
        )
    }

    #[test]
    /// Inserting 5 log lines in { file_a }:
    /// [
    ///     this is the first line.
    ///     this is the second line.
    ///     this is the third line.
    ///     this is the fourth line.
    ///     this is the fifth line.
    /// ]
    ///
    /// Now returns all lines since filtering is done by the agent.
    fn test_read_log_content() {
        let (tempdir, _) = prepare();
        let tempdir_path = tempdir.path().to_str().unwrap();
        let file_path = &format!("{tempdir_path}/file_a_one");
        let mut log_file = std::fs::OpenOptions::new()
            .append(true)
            .create(false)
            .open(file_path)
            .unwrap();

        let data = "this is the first line.\nthis is the second line.\nthis is the third line.\nthis is the forth line.\nthis is the fifth line.";

        log_file.write_all(data.as_bytes()).unwrap();
        log_file.flush().unwrap();

        let result = read_log_content(Path::new(file_path)).unwrap();

        assert_eq!(result, "filename: file_a_one\nthis is the first line.\nthis is the second line.\nthis is the third line.\nthis is the forth line.\nthis is the fifth line.\n");
    }

    #[test]
    /// Inserting 5 lines of logs for each log file { file_a, ..., file_d }.
    /// Each line contains the text: "this is the { line_number } line of { file_name }
    /// where line_number { first, second, third, forth, fifth }
    /// where file_name { file_a, ..., file_d }
    ///
    /// Requesting logs for log_type = "type_one", that are older than:
    /// timestamp: 1970/01/01 00:00:03
    ///
    /// These are:
    /// file_b and file_d
    ///
    /// file_d is the newest file, so its logs are read first. then file_b.
    ///
    /// Now returns all lines since filtering is done by the agent.
    fn test_read_log_content_multiple_files() {
        let (tempdir, files) = prepare();
        let tempdir_path = tempdir.path().to_str().unwrap();

        for (file_name, m_time) in [
            ("file_a_one", 2),
            ("file_b_one", 3),
            ("file_c_two", 11),
            ("file_d_one", 100),
        ] {
            let file_path = &format!("{tempdir_path}/{file_name}");

            let mut log_file = std::fs::OpenOptions::new()
                .append(true)
                .create(false)
                .open(file_path)
                .unwrap();

            let data = &format!("this is the first line of {file_name}.\nthis is the second line of {file_name}.\nthis is the third line of {file_name}.\nthis is the forth line of {file_name}.\nthis is the fifth line of {file_name}.");

            log_file.write_all(data.as_bytes()).unwrap();
            log_file.flush().unwrap();

            let new_mtime = FileTime::from_unix_time(m_time, 0);
            set_file_mtime(file_path, new_mtime).unwrap();
        }
        let temp_path = new_read_logs(
            &files,
            "type_one",
            datetime!(1970-01-01 00:00:03 +00:00),
            tempdir.utf8_path(),
        )
        .unwrap();

        assert_eq!(temp_path.parent().unwrap(), tempdir.path());

        let result = std::fs::read_to_string(temp_path).unwrap();
        assert_eq!(result, String::from("filename: file_d_one\nthis is the first line of file_d_one.\nthis is the second line of file_d_one.\nthis is the third line of file_d_one.\nthis is the forth line of file_d_one.\nthis is the fifth line of file_d_one.\nfilename: file_b_one\nthis is the first line of file_b_one.\nthis is the second line of file_b_one.\nthis is the third line of file_b_one.\nthis is the forth line of file_b_one.\nthis is the fifth line of file_b_one.\n"))
    }
}
