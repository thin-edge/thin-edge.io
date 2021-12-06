use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use crate::error::OperationsError;

/// Operations are derived by reading files subdirectories per cloud /etc/tedge/operations directory
/// Each operation is a file name in one of the subdirectories
/// The file name is the operation name

type Cloud = String;
type OperationName = String;
type Operation = HashSet<OperationName>;
type OperationsMap = HashMap<Cloud, Operation>;

#[derive(Debug, Clone, PartialEq)]
pub struct Operations {
    dir: PathBuf,
    operations: OperationsMap,
}

impl Operations {
    pub fn try_new(dir: impl AsRef<Path>) -> Result<Self, OperationsError> {
        let operations = get_operations(dir.as_ref())?;

        Ok(Self {
            dir: dir.as_ref().to_path_buf(),
            operations,
        })
    }

    pub fn get_operations_list(&self, cloud: &str) -> Vec<&str> {
        self.operations
            .get(cloud)
            .map(|operations| operations.iter().map(|k| k.as_str()).collect())
            .unwrap_or_default()
    }
}

fn get_clouds(dir: impl AsRef<Path>) -> Result<Vec<String>, OperationsError> {
    Ok(fs::read_dir(dir)?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<Result<Vec<PathBuf>, _>>()?
        .into_iter()
        .filter(|path| path.is_dir())
        .map(|path| {
            let filename = path.file_name();
            filename.unwrap().to_str().unwrap().to_string()
        })
        .collect())
}

fn get_operations(dir: impl AsRef<Path>) -> Result<OperationsMap, OperationsError> {
    let mut operations = OperationsMap::new();
    for cloud in get_clouds(&dir)? {
        let path = dir.as_ref().join(cloud.as_str());
        let operations_map = fs::read_dir(&path)?
            .map(|entry| entry.map(|e| e.path()))
            .collect::<Result<Vec<PathBuf>, _>>()?
            .into_iter()
            .filter(|path| path.is_file())
            .map(|path| {
                let filename = path.file_name();
                filename.unwrap().to_str().unwrap().to_string()
            })
            .collect();
        operations.insert(cloud, operations_map);
    }
    Ok(operations)
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[test_case(0, false)]
    #[test_case(0, true)]
    #[test_case(2, false)]
    #[test_case(2, true)]
    fn get_clouds_tests(clouds_count: usize, files: bool) {
        let operations = TestOperations::builder().with_clouds(clouds_count);

        if files {
            operations.with_random_file_in_clouds_directory();
        }

        let operations = operations.build();

        let clouds = get_clouds(operations.temp_dir()).unwrap();

        assert_eq!(clouds.len(), clouds_count);
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

        let operations = get_operations(test_operations.temp_dir()).unwrap();

        assert_eq!(operations.len(), clouds_count);
        assert_eq!(
            operations.values().map(|ops| ops.len()).sum::<usize>(),
            ops_count * clouds_count
        );
    }

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
                    fs::File::create(&file_path).unwrap();
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
}
