use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use serde::Deserialize;

use crate::error::OperationsError;

/// Operations are derived by reading files subdirectories per cloud /etc/tedge/operations directory
/// Each operation is a file name in one of the subdirectories
/// The file name is the operation name

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct Exec {
    // name: Option<OperationName>,
    on_message: Option<String>,
    command: Option<String>,
    // args: Option<Vec<String>>,
    user: Option<String>,
    topic: Option<String>,
}

impl Exec {
    fn get_on_message(&self) -> Option<&str> {
        self.on_message.as_deref()
    }

    /// Get a reference to the exec's command.
    pub fn command(&self) -> Option<&String> {
        self.command.as_ref()
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub struct Operation {
    #[serde(skip)]
    name: String,
    exec: Option<Exec>,
}

impl Operation {
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn exec(&self) -> Option<&Exec> {
        self.exec.as_ref()
    }

    pub fn command(&self) -> Option<String> {
        self.exec().and_then(|exec| exec.command.clone())
    }

    pub fn topic(&self) -> Option<String> {
        self.exec().and_then(|exec| exec.topic.clone())
    }
}

#[derive(Debug, Clone)]
pub struct Operations {
    operations: Vec<Operation>,
    operations_by_trigger: HashMap<String, usize>,
}

impl Operations {
    pub fn new() -> Self {
        Self {
            operations: vec![],
            operations_by_trigger: HashMap::new(),
        }
    }

    pub fn add(&mut self, operation: Operation) {
        if let Some(detail) = operation.exec() {
            if let Some(on_message) = &detail.on_message {
                self.operations_by_trigger
                    .insert(on_message.clone(), self.operations.len());
            }
        }
        self.operations.push(operation);
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

    pub fn topics_for_operations(&self) -> Vec<String> {
        self.operations
            .iter()
            .filter_map(|operation| operation.topic())
            .collect::<Vec<String>>()
    }
}

fn get_operations(dir: impl AsRef<Path>, cloud_name: &str) -> Result<Operations, OperationsError> {
    let mut operations = Operations::new();

    let path = dir.as_ref().join(&cloud_name);
    let dir_entries = fs::read_dir(&path)?
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

        operations.add(details);
    }
    Ok(operations)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;
    use test_case::test_case;

    // Structs for state change with the builder pattern
    // Structs for Clouds
    struct Clouds(Vec<PathBuf>);
    struct NoClouds;

    // Structs for Operations
    struct Ops(Vec<PathBuf>);
    struct NoOps;

    struct TestOperationsBuilder<C, O> {
        temp_dir: tempfile::TempDir,
        clouds: C,
        operations: O,
    }

    impl TestOperationsBuilder<NoClouds, NoOps> {
        fn new() -> Self {
            Self {
                temp_dir: tempfile::tempdir().unwrap(),
                clouds: NoClouds,
                operations: NoOps,
            }
        }
    }

    impl<O> TestOperationsBuilder<NoClouds, O> {
        fn with_clouds(self, clouds_count: usize) -> TestOperationsBuilder<Clouds, O> {
            let Self {
                temp_dir,
                operations,
                ..
            } = self;

            let mut clouds = Vec::new();
            for i in 0..clouds_count {
                let cloud = temp_dir.as_ref().join(format!("cloud{}", i));
                fs::create_dir(&cloud).unwrap();
                clouds.push(cloud);
            }

            TestOperationsBuilder {
                temp_dir,
                clouds: Clouds(clouds),
                operations,
            }
        }
    }

    impl TestOperationsBuilder<Clouds, NoOps> {
        fn with_operations(self, operations_count: usize) -> TestOperationsBuilder<Clouds, Ops> {
            let Self {
                temp_dir, clouds, ..
            } = self;

            let mut operations = Vec::new();
            clouds.0.iter().for_each(|path| {
                for i in 0..operations_count {
                    let file_path = path.join(format!("operation{}", i));
                    let mut file = fs::File::create(&file_path).unwrap();
                    file.write_all(
                        br#"[exec]
                        command = "echo"
                        on_message = "511""#,
                    )
                    .unwrap();
                    operations.push(file_path);
                }
            });

            TestOperationsBuilder {
                operations: Ops(operations),
                temp_dir,
                clouds,
            }
        }

        fn build(self) -> TestOperations {
            let Self {
                temp_dir, clouds, ..
            } = self;

            TestOperations {
                temp_dir,
                clouds: clouds.0,
                operations: Vec::new(),
            }
        }
    }

    impl<C, O> TestOperationsBuilder<C, O> {
        fn with_random_file_in_clouds_directory(&self) {
            let path = self.temp_dir.as_ref().join("cloudfile");
            fs::File::create(path).unwrap();
        }
    }

    impl TestOperationsBuilder<Clouds, Ops> {
        fn build(self) -> TestOperations {
            let Self {
                temp_dir,
                clouds,
                operations,
            } = self;

            TestOperations {
                temp_dir,
                clouds: clouds.0,
                operations: operations.0,
            }
        }
    }

    struct TestOperations {
        temp_dir: tempfile::TempDir,
        clouds: Vec<PathBuf>,
        operations: Vec<PathBuf>,
    }

    impl TestOperations {
        fn builder() -> TestOperationsBuilder<NoClouds, NoOps> {
            TestOperationsBuilder::new()
        }

        fn temp_dir(&self) -> &tempfile::TempDir {
            &self.temp_dir
        }

        fn operations(&self) -> &Vec<PathBuf> {
            &self.operations
        }
    }

    #[test_case(0, 0)]
    #[test_case(1, 1)]
    #[test_case(1, 5)]
    #[test_case(2, 5)]
    fn get_operations_all(clouds_count: usize, ops_count: usize) {
        let test_operations = TestOperations::builder()
            .with_clouds(clouds_count)
            .with_operations(ops_count)
            .build();

        let operations = get_operations(test_operations.temp_dir(), "").unwrap();
        dbg!(&operations);

        assert_eq!(operations.operations.len(), ops_count);
    }
}
