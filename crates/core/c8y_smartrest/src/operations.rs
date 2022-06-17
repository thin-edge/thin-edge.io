use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use crate::error::OperationsError;
use serde::Deserialize;

/// Operations are derived by reading files subdirectories per cloud /etc/tedge/operations directory
/// Each operation is a file name in one of the subdirectories
/// The file name is the operation name

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct OnMessageExec {
    command: Option<String>,
    on_message: Option<String>,
    topic: Option<String>,
    user: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
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
}

#[derive(Debug, Default, Clone)]
pub struct Operations {
    operations: Vec<Operation>,
    operations_by_trigger: HashMap<String, usize>,
}

impl Operations {
    pub fn add_operation(&mut self, operation: Operation) {
        if self.operations.iter().any(|o| o.name.eq(&operation.name)) {
        } else {
            if let Some(detail) = operation.exec() {
                if let Some(on_message) = &detail.on_message {
                    self.operations_by_trigger
                        .insert(on_message.clone(), self.operations.len());
                }
            }
            self.operations.push(operation);
        }
    }

    pub fn remove_operation(&mut self, op_name: &str) {
        self.operations.retain(|x| x.name.ne(&op_name));
    }

    pub fn try_new(dir: impl AsRef<Path>, cloud_name: &str) -> Result<Self, OperationsError> {
        get_operations(dir.as_ref(), cloud_name)
    }

    pub fn get_operations_list(&self) -> Vec<String> {
        self.operations
            .iter()
            .map(|operation| operation.name.clone())
            .collect::<Vec<String>>()
    }

    pub fn matching_smartrest_template(&self, operation_template: &str) -> Option<&Operation> {
        self.operations_by_trigger
            .get(operation_template)
            .and_then(|index| self.operations.get(*index))
    }

    pub fn topics_for_operations(&self) -> HashSet<String> {
        self.operations
            .iter()
            .filter_map(|operation| operation.topic())
            .collect::<HashSet<String>>()
    }
}

fn get_operations(dir: impl AsRef<Path>, cloud_name: &str) -> Result<Operations, OperationsError> {
    let mut operations = Operations::default();

    let path = dir.as_ref().join(&cloud_name);
    let dir_entries = fs::read_dir(&path)
        .map_err(|_| OperationsError::ReadDirError {
            dir: PathBuf::from(&path),
        })?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<Result<Vec<PathBuf>, _>>()?
        .into_iter()
        .filter(|path| path.is_file())
        .collect::<Vec<PathBuf>>();

    for path in dir_entries {
        let mut details = match fs::read(&path) {
            Ok(bytes) => toml::from_slice::<Operation>(bytes.as_slice())
                .map_err(|e| OperationsError::TomlError(path.to_path_buf(), e))?,

            Err(err) => return Err(OperationsError::FromIo(err)),
        };

        details.name = path
            .file_name()
            .and_then(|filename| filename.to_str())
            .ok_or_else(|| OperationsError::InvalidOperationName(path.to_owned()))?
            .to_owned();
        operations.add_operation(details);
    }
    Ok(operations)
}

pub fn get_operation(path: PathBuf) -> Result<Operation, OperationsError> {
    let mut details = match fs::read(&path) {
        Ok(bytes) => toml::from_slice::<Operation>(bytes.as_slice())
            .map_err(|e| OperationsError::TomlError(path.to_path_buf(), e))?,

        Err(err) => return Err(OperationsError::FromIo(err)),
    };

    details.name = path
        .file_name()
        .and_then(|filename| filename.to_str())
        .ok_or_else(|| OperationsError::InvalidOperationName(path.to_owned()))?
        .to_owned();

    Ok(details)
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

    #[test_case(0)]
    #[test_case(1)]
    #[test_case(5)]
    fn get_operations_all(ops_count: usize) {
        let test_operations = TestOperations::builder().with_operations(ops_count).build();

        let operations = get_operations(test_operations.temp_dir(), "").unwrap();
        dbg!(&operations);

        assert_eq!(operations.operations.len(), ops_count);
    }
}
