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
        .collect::<Result<Vec<_>, _>>()?
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
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|path| path.is_file())
            .map(|path| {
                (
                    {
                        let filename = path.file_name();
                        dbg!(filename);
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
    fn get_clouds_2(count: usize, files: bool) {
        let temp_dir = tempfile::tempdir().unwrap();
        let dir = temp_dir.path();

        create_clouds_directories(count, dir, files).unwrap();

        let clouds = get_clouds(dir).unwrap();
        dbg!(&clouds);

        assert_eq!(clouds.len(), count);
    }

    #[test]
    fn get_operations_all() {
        // let temp_dir = tempfile::tempdir().unwrap();
        // let dir = temp_dir.path();
        let dir = Path::new("/home/user/tedge/operations/");

        // create_clouds_directories(2, dir, true).unwrap();

        let operations = get_operations(dir).unwrap();
        dbg!(&operations);

        assert_eq!(operations.len(), 2);
    }

    fn create_clouds_directories(
        count: usize,
        dir: impl AsRef<Path>,
        random_file: bool,
    ) -> Result<(), OperationsError> {
        for i in 0..count {
            let path = dir.as_ref().join(format!("cloud{}", i));
            fs::create_dir(path)?;
        }

        if random_file {
            let path = dir.as_ref().join("cloudfile");
            fs::File::create(path)?;
        }

        Ok(())
    }

    fn crate_operations_files(count: usize, dir: impl AsRef<Path>) -> Result<(), OperationsError> {
        for i in 0..count {
            let path = dir.as_ref().join(format!("operation{}", i));
            fs::File::create(path).unwrap();
        }

        Ok(())
    }
}
