use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use crate::error::OperationsError;

/// Operations are derived by reading files subdirectories per cloud /etc/tedge/operations directory
/// Each operation is a file name in one of the subdirectories
/// The file name is the operation name

type Cloud = String;
type OperationName = String;
type Operation = HashMap<OperationName, PathBuf>;
type OperationsMap = HashMap<Cloud, Operation>;

#[derive(Debug, Clone, PartialEq)]
pub struct Operations {
    dir: PathBuf,
    operations: OperationsMap,
}

impl Default for Operations {
    fn default() -> Self {
        Self {
            dir: Path::new("/etc/tedge/operations").to_path_buf(),
            operations: HashMap::new(),
        }
    }
}

impl Operations {
    pub fn new(dir: impl AsRef<Path>) -> Self {
        let operations = get_operations(&dir.as_ref()).unwrap_or_default();
        Self {
            dir: dir.as_ref().to_path_buf(),
            operations,
        }
    }

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
            .map(|operations| operations.keys().map(|k| k.as_str()).collect())
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
            dbg!(filename);
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
                (
                    {
                        let filename = path.file_name();
                        filename.unwrap().to_str().unwrap().to_string()
                    },
                    path,
                )
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
        let operations = TestOperations::new().with_clouds(clouds_count);

        if files {
            operations.with_random_file_in_clouds_directory();
        }

        let clouds = get_clouds(operations.temp_dir()).unwrap();

        assert_eq!(clouds.len(), clouds_count);
    }

    #[test_case(0, 0)]
    #[test_case(1, 1)]
    #[test_case(1, 5)]
    #[test_case(2, 5)]
    fn get_operations_all(clouds_count: usize, ops_count: usize) {
        let test_operations = TestOperations::new()
            .with_clouds(clouds_count)
            .with_operations(ops_count);

        let operations = get_operations(test_operations.temp_dir()).unwrap();

        assert_eq!(operations.len(), clouds_count);
        assert_eq!(
            operations.values().map(|ops| ops.len()).sum::<usize>(),
            ops_count * clouds_count
        );
    }

    struct TestOperations {
        temp_dir: tempfile::TempDir,
        clouds: Vec<PathBuf>,
        operations: Vec<PathBuf>,
    }

    impl TestOperations {
        fn new() -> Self {
            Self {
                temp_dir: tempfile::tempdir().unwrap(),
                clouds: Vec::new(),
                operations: Vec::new(),
            }
        }

        fn with_clouds(self, clouds_count: usize) -> Self {
            let mut clouds = Vec::new();
            for i in 0..clouds_count {
                let cloud = self.temp_dir.as_ref().join(format!("cloud{}", i));
                fs::create_dir(&cloud).unwrap();
                clouds.push(cloud);
            }

            Self { clouds, ..self }
        }

        fn with_operations(self, operations_count: usize) -> Self {
            let mut operations = Vec::new();
            self.clouds.iter().for_each(|path| {
                dbg!(&path);
                for i in 0..operations_count {
                    let file_path = path.join(format!("operation{}", i));
                    fs::File::create(&file_path).unwrap();
                    operations.push(file_path);
                }
            });

            Self { operations, ..self }
        }

        fn with_random_file_in_clouds_directory(&self) {
            let path = self.temp_dir.as_ref().join("cloudfile");
            fs::File::create(path).unwrap();
        }

        fn temp_dir(&self) -> &tempfile::TempDir {
            &self.temp_dir
        }

        fn operations(&self) -> &Vec<PathBuf> {
            &self.operations
        }
    }
}
