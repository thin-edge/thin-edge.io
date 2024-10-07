use crate::smartrest::error::OperationsError;
use crate::smartrest::smartrest_serializer::declare_supported_operations;
use serde::Deserialize;
use serde::Deserializer;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use tracing::warn;

const DEFAULT_GRACEFUL_TIMEOUT: Duration = Duration::from_secs(3600);
const DEFAULT_FORCEFUL_TIMEOUT: Duration = Duration::from_secs(60);

/// Operations are derived by reading files subdirectories per cloud /etc/tedge/operations directory
/// Each operation is a file name in one of the subdirectories
/// The file name is the operation name
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Operations {
    operations: Vec<Operation>,
}

impl Operations {
    pub fn add_operation(&mut self, operation: Operation) {
        self.operations.push(operation);
    }

    pub fn try_new(dir: impl AsRef<Path>) -> Result<Self, OperationsError> {
        get_operations(dir.as_ref())
    }

    pub fn get_operations_list(&self) -> Vec<String> {
        let mut ops_name: Vec<String> = Vec::default();
        for op in &self.operations {
            ops_name.push(op.name.clone());
        }

        ops_name
    }

    pub fn matching_smartrest_template(&self, operation_template: &str) -> Option<Operation> {
        for op in self.operations.clone() {
            if let Some(template) = op.template() {
                if template.eq(operation_template) {
                    return Some(op);
                }
            }
        }
        None
    }

    pub fn filter_by_topic(&self, topic_name: &str) -> Vec<(String, Operation)> {
        let mut vec: Vec<(String, Operation)> = Vec::new();
        for op in self.operations.iter() {
            match (op.topic(), op.on_fragment()) {
                (None, Some(on_fragment)) => vec.push((on_fragment, op.clone())),
                (Some(topic), Some(on_fragment)) if topic == topic_name => {
                    vec.push((on_fragment, op.clone()))
                }
                _ => {}
            }
        }
        vec
    }

    pub fn topics_for_operations(&self) -> HashSet<String> {
        self.operations
            .iter()
            .filter_map(|operation| operation.topic())
            .collect::<HashSet<String>>()
    }

    pub fn create_smartrest_ops_message(&self) -> String {
        let mut ops = self.get_operations_list();
        ops.sort();
        let ops = ops.iter().map(|op| op.as_str()).collect::<Vec<_>>();
        declare_supported_operations(&ops)
    }
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub struct Operation {
    #[serde(skip)]
    pub name: String,
    exec: Option<OnMessageExec>,
}

impl Operation {
    pub fn exec(&self) -> Option<&OnMessageExec> {
        self.exec.as_ref()
    }

    pub fn command(&self) -> Option<String> {
        self.exec().and_then(|exec| exec.command.clone())
    }

    pub fn topic(&self) -> Option<String> {
        self.exec().and_then(|exec| exec.topic.clone())
    }

    pub fn on_message(&self) -> Option<String> {
        self.exec().and_then(|exec| exec.on_message.clone())
    }

    pub fn on_fragment(&self) -> Option<String> {
        self.exec().and_then(|exec| exec.on_fragment.clone())
    }

    pub fn skip_status_update(&self) -> bool {
        self.exec().unwrap().skip_status_update
    }

    pub fn result_format(&self) -> ResultFormat {
        self.exec()
            .map(|exec| exec.result_format.clone())
            .unwrap_or_default()
    }

    pub fn template(&self) -> Option<String> {
        self.exec().and_then(|exec| exec.on_message.clone())
    }

    pub fn graceful_timeout(&self) -> Duration {
        self.exec()
            .map(|exec| exec.graceful_timeout)
            .unwrap_or(DEFAULT_GRACEFUL_TIMEOUT)
    }

    pub fn forceful_timeout(&self) -> Duration {
        self.exec()
            .map(|exec| exec.forceful_timeout)
            .unwrap_or(DEFAULT_FORCEFUL_TIMEOUT)
    }

    fn is_supported_operation_file(&self) -> bool {
        self.exec().is_none()
    }

    fn is_valid_operation_handler(&self) -> bool {
        if self.exec.is_none() {
            return false;
        }
        if let Err(err) = self.validate_smartrest_operation_handler() {
            warn!(
                "'{err} in the SmartREST custom operation handler mapping '{name}'",
                name = self.name
            );
            return false;
        }
        if let Err(err) = self.validate_json_operation_handler() {
            warn!(
                "'{err} in the JSON custom operation handler mapping '{name}'",
                name = self.name
            );
            return false;
        }

        true
    }

    fn validate_smartrest_operation_handler(&self) -> Result<(), InvalidCustomOperationHandler> {
        if self.on_message().is_some() {
            if self.topic().is_none() {
                return Err(InvalidCustomOperationHandler::MissingTopic);
            }
            if self.on_fragment().is_some() {
                return Err(InvalidCustomOperationHandler::OnFragmentExists);
            }
            if self.command().is_none() {
                return Err(InvalidCustomOperationHandler::MissingCommand);
            }
        }
        Ok(())
    }

    fn validate_json_operation_handler(&self) -> Result<(), InvalidCustomOperationHandler> {
        if self.on_message().is_none() {
            if self.on_fragment().is_none() {
                return Err(InvalidCustomOperationHandler::MissingOnFragment);
            }
            if self.command().is_none() {
                return Err(InvalidCustomOperationHandler::MissingCommand);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct OnMessageExec {
    command: Option<String>,
    on_message: Option<String>,
    on_fragment: Option<String>,
    topic: Option<String>,
    user: Option<String>,
    #[serde(default)]
    skip_status_update: bool,
    #[serde(default, deserialize_with = "to_result_format")]
    result_format: ResultFormat,
    #[serde(rename = "timeout")]
    #[serde(default = "default_graceful_timeout", deserialize_with = "to_duration")]
    pub graceful_timeout: Duration,
    #[serde(default = "default_forceful_timeout", deserialize_with = "to_duration")]
    pub forceful_timeout: Duration,
}

impl OnMessageExec {
    pub fn set_time_out(&mut self, timeout: Duration) {
        self.graceful_timeout = timeout;
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub enum ResultFormat {
    #[default]
    Text,
    Csv,
}

fn to_result_format<'de, D>(deserializer: D) -> Result<ResultFormat, D::Error>
where
    D: Deserializer<'de>,
{
    let val: String = serde::Deserialize::deserialize(deserializer)?;

    match val.as_str() {
        "text" => Ok(ResultFormat::Text),
        "csv" => Ok(ResultFormat::Csv),
        _ => Err(serde::de::Error::unknown_variant(&val, &["text", "csv"])),
    }
}

pub fn get_operations(dir: impl AsRef<Path>) -> Result<Operations, OperationsError> {
    let mut operations = Operations::default();
    let dir_entries = fs::read_dir(&dir)
        .map_err(|_| OperationsError::ReadDirError {
            dir: dir.as_ref().to_path_buf(),
        })?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<Result<Vec<PathBuf>, _>>()?
        .into_iter()
        .filter(|path| path.is_file())
        .collect::<Vec<PathBuf>>();

    for path in dir_entries {
        if let Some(file_name) = path.file_name().and_then(|file_name| file_name.to_str()) {
            if !is_valid_operation_name(file_name) {
                // Warn user about invalid operation names, otherwise the
                // user does not know that the operation is being ignored
                warn!("Ignoring custom operation definition as the filename uses an invalid character. Only [A-Za-z0-9_] characters are accepted. file={}", path.display());
                continue;
            }

            let mut details = match fs::read(&path) {
                Ok(bytes) => toml::from_str::<Operation>(&String::from_utf8(bytes)?)
                    .map_err(|e| OperationsError::TomlError(path.to_path_buf(), e))?,
                Err(err) => return Err(OperationsError::FromIo(err)),
            };

            path.file_name()
                .and_then(|filename| filename.to_str())
                .ok_or_else(|| OperationsError::InvalidOperationName(path.to_owned()))?
                .clone_into(&mut details.name);

            if details.is_valid_operation_handler() || details.is_supported_operation_file() {
                operations.add_operation(details);
            }
        }
    }
    Ok(operations)
}

pub fn get_child_ops(
    ops_dir: impl AsRef<Path>,
) -> Result<HashMap<String, Operations>, OperationsError> {
    let mut child_ops: HashMap<String, Operations> = HashMap::new();
    let child_entries = fs::read_dir(&ops_dir)
        .map_err(|_| OperationsError::ReadDirError {
            dir: ops_dir.as_ref().into(),
        })?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<Result<Vec<PathBuf>, _>>()?
        .into_iter()
        .filter(|path| path.is_dir())
        .collect::<Vec<PathBuf>>();
    for cdir in child_entries {
        let ops = Operations::try_new(&cdir)?;
        if let Some(id) = cdir.file_name() {
            if let Some(id_str) = id.to_str() {
                child_ops.insert(id_str.to_string(), ops);
            }
        }
    }
    Ok(child_ops)
}

pub fn get_operation(path: PathBuf) -> Result<Operation, OperationsError> {
    let mut details = match fs::read(&path) {
        Ok(bytes) => toml::from_str::<Operation>(&String::from_utf8(bytes)?)
            .map_err(|e| OperationsError::TomlError(path.to_path_buf(), e))?,

        Err(err) => return Err(OperationsError::FromIo(err)),
    };

    path.file_name()
        .and_then(|filename| filename.to_str())
        .ok_or_else(|| OperationsError::InvalidOperationName(path.to_owned()))?
        .clone_into(&mut details.name);

    Ok(details)
}

/// depending on which editor you use, temporary files could be created that contain the name of
/// the file.
/// this `operation_name_is_valid` fn will ensure that only files that do not contain
/// any special characters are allowed.
pub fn is_valid_operation_name(operation: &str) -> bool {
    operation
        .chars()
        .all(|c| c.is_ascii_alphabetic() || c.is_numeric() || c.eq(&'_'))
}

fn default_graceful_timeout() -> Duration {
    DEFAULT_GRACEFUL_TIMEOUT
}

fn default_forceful_timeout() -> Duration {
    DEFAULT_FORCEFUL_TIMEOUT
}

fn to_duration<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    let timeout = Deserialize::deserialize(deserializer)?;
    Ok(Duration::from_secs(timeout))
}

/// Invalid mapping definition of custom operation handlers
#[derive(thiserror::Error, Debug)]
pub enum InvalidCustomOperationHandler {
    #[error("'topic' is missing'")]
    MissingTopic,

    #[error("'on_fragment' should not be provided for SmartREST custom operation handler")]
    OnFragmentExists,

    #[error("'on_fragment' is missing")]
    MissingOnFragment,

    #[error("'command' is missing")]
    MissingCommand,
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;
    use test_case::test_case;

    // Structs for state change with the builder pattern
    // Structs for Operations
    struct Ops(Vec<PathBuf>);
    struct NoOps;

    struct TestOperationsBuilder<O> {
        temp_dir: tempfile::TempDir,
        operations: O,
    }

    impl TestOperationsBuilder<NoOps> {
        fn new() -> Self {
            Self {
                temp_dir: tempfile::tempdir().unwrap(),
                operations: NoOps,
            }
        }
    }

    impl TestOperationsBuilder<NoOps> {
        fn with_operations(self, operations_count: usize) -> TestOperationsBuilder<Ops> {
            let Self { temp_dir, .. } = self;

            let mut operations = Vec::new();
            for i in 0..operations_count {
                let file_path = temp_dir.path().join(format!("operation{}", i));
                let mut file = fs::File::create(&file_path).unwrap();
                file.write_all(
                    br#"[exec]
                        topic = "c8y/s/us"
                        command = "echo"
                        on_message = "511""#,
                )
                .unwrap();
                operations.push(file_path);
            }

            TestOperationsBuilder {
                operations: Ops(operations),
                temp_dir,
            }
        }
    }

    impl TestOperationsBuilder<Ops> {
        fn build(self) -> TestOperations {
            let Self {
                temp_dir,
                operations,
            } = self;

            TestOperations {
                temp_dir,
                operations: operations.0,
            }
        }
    }

    struct TestOperations {
        temp_dir: tempfile::TempDir,
        #[allow(dead_code)]
        operations: Vec<PathBuf>,
    }

    impl TestOperations {
        fn builder() -> TestOperationsBuilder<NoOps> {
            TestOperationsBuilder::new()
        }

        fn temp_dir(&self) -> &tempfile::TempDir {
            &self.temp_dir
        }
    }

    impl Operation {
        fn new(exec: OnMessageExec) -> Self {
            Self {
                name: "name".to_string(),
                exec: Some(exec),
            }
        }
    }

    #[test_case(0)]
    #[test_case(1)]
    #[test_case(5)]
    fn get_operations_all(ops_count: usize) {
        let test_operations = TestOperations::builder().with_operations(ops_count).build();

        let operations = get_operations(test_operations.temp_dir()).unwrap();

        assert_eq!(operations.operations.len(), ops_count);
    }

    #[test_case("file_a?", false)]
    #[test_case("~file_b", false)]
    #[test_case("c8y_Command", true)]
    #[test_case("c8y_CommandA~", false)]
    #[test_case(".c8y_CommandB", false)]
    #[test_case("c8y_CommandD?", false)]
    #[test_case("c8y_CommandE?!£$%^&*(", false)]
    #[test_case("?!£$%^&*(c8y_CommandF?!£$%^&*(", false)]
    fn operation_name_should_contain_only_alphabetic_chars(operation: &str, expected_result: bool) {
        assert_eq!(is_valid_operation_name(operation), expected_result)
    }

    #[test]
    fn deserialize_result_format() {
        let toml: OnMessageExec = toml::from_str(r#"result_format = "csv""#).unwrap();
        assert_eq!(toml.result_format, ResultFormat::Csv);

        let toml: OnMessageExec = toml::from_str(r#"result_format = "text""#).unwrap();
        assert_eq!(toml.result_format, ResultFormat::Text);

        let toml: OnMessageExec = toml::from_str("").unwrap();
        assert_eq!(toml.result_format, ResultFormat::Text);

        let result = toml::from_str::<OnMessageExec>(r#"result_format = "foo""#);
        assert!(result.is_err());
    }

    #[test_case(
        r#"
        topic = "c8y/s/us"
        on_message = "123"
        command = "echo"
        "#
    )]
    #[test_case(
        r#"
        on_fragment = "c8y_Something"
        command = "echo"
        "#
    )]
    #[test_case(
        r#"
        topic = "c8y/devicecontrol/notifications"
        on_fragment = "c8y_Something"
        command = "echo"
        "#
    )]
    fn valid_custom_operation_handlers(toml: &str) {
        let exec: OnMessageExec = toml::from_str(toml).unwrap();
        let operation = Operation::new(exec);
        assert!(operation.is_valid_operation_handler());
    }

    #[test_case(
        r#"
        on_message = "123"
        command = "echo"
        "#
    )]
    #[test_case(
        r#"
        topic = "c8y/s/us"
        on_message = "123"
        "#
    )]
    #[test_case(
        r#"
        command = "echo"
        "#
    )]
    #[test_case(
        r#"
        topic = "c8y/devicecontrol/notifications"
        on_message = "1234"
        on_fragment = "c8y_Something"
        command = "echo"
        "#
    )]
    fn invalid_custom_operation_handlers(toml: &str) {
        let exec: OnMessageExec = toml::from_str(toml).unwrap();
        let operation = Operation::new(exec);
        assert!(!operation.is_valid_operation_handler());
    }
}
