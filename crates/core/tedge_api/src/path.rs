use camino::Utf8Path;
use tedge_utils::paths::ManagedDir;
use tedge_utils::paths::PathsError;
use tedge_utils::paths::TedgePaths;

/// Root directory for thin-edge persistent data (`data.path`, default `/var/tedge`).
///
/// Wraps [`TedgePaths`] and provides named accessors for the well-known
/// subdirectories that live under the data root.
#[derive(Debug, Clone)]
pub struct DataDir(TedgePaths);

impl From<TedgePaths> for DataDir {
    fn from(paths: TedgePaths) -> Self {
        DataDir(paths)
    }
}

impl DataDir {
    pub fn root(&self) -> &Utf8Path {
        self.0.root()
    }

    pub fn root_dir(&self) -> ManagedDir {
        self.0.root_dir()
    }

    pub fn firmware_dir(&self) -> ManagedDir {
        self.0
            .dir("firmware")
            .expect("'firmware' is a valid relative path")
    }

    pub fn cache_dir(&self) -> ManagedDir {
        self.0
            .dir("cache")
            .expect("'cache' is a valid relative path")
    }

    pub fn file_transfer_dir(&self) -> ManagedDir {
        self.0
            .dir("file-transfer")
            .expect("'file-transfer' is a valid relative path")
    }

    pub fn dir(&self, path: impl AsRef<Utf8Path>) -> Result<ManagedDir, PathsError> {
        self.0.dir(path)
    }
}
