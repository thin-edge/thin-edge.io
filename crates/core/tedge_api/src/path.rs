use camino::Utf8Path;
use camino::Utf8PathBuf;
use std::path::Path;

/// Representation of ThinEdge data directory.
/// Default is /var/tedge.
/// All directories under the root data directory must be listed here.
#[derive(Debug, Clone)]
pub struct DataDir(Utf8PathBuf);

impl Default for DataDir {
    fn default() -> Self {
        DataDir("/var/tedge".into())
    }
}

impl From<Utf8PathBuf> for DataDir {
    fn from(path: Utf8PathBuf) -> Self {
        DataDir(path)
    }
}

impl From<DataDir> for Utf8PathBuf {
    fn from(value: DataDir) -> Self {
        value.0
    }
}

impl AsRef<Path> for DataDir {
    fn as_ref(&self) -> &Path {
        self.0.as_std_path()
    }
}

impl DataDir {
    /// Crete `DataDir` from `Utf8PathBuf`.
    pub fn new(path: Utf8PathBuf) -> Self {
        DataDir::from(path)
    }

    /// Creates an owned `Utf8PathBuf` with `path` adjoined to `self`.
    ///
    /// # Examples
    ///
    /// ```
    /// use camino::Utf8PathBuf;
    /// use tedge_api::path::DataDir;
    ///
    /// assert_eq!(DataDir::default().join("foo"), Utf8PathBuf::from("/var/tedge/foo"));
    /// ```
    pub fn join(&self, path: impl AsRef<Utf8Path>) -> Utf8PathBuf {
        self.0.join(path.as_ref().to_path_buf())
    }

    /// Return `Utf8PathBuf` to ThinEdge file transfer repository.
    ///
    /// # Examples
    ///
    /// ```
    /// use camino::Utf8PathBuf;
    /// use tedge_api::path::DataDir;
    ///
    /// assert_eq!(DataDir::default().file_transfer_dir(), Utf8PathBuf::from("/var/tedge/file-transfer"));
    /// ```
    pub fn file_transfer_dir(&self) -> Utf8PathBuf {
        self.0.join("file-transfer")
    }

    /// Return `Utf8PathBuf` to ThinEdge file cache repository.
    ///
    /// # Examples
    ///
    /// ```
    /// use camino::Utf8PathBuf;
    /// use tedge_api::path::DataDir;
    ///
    /// assert_eq!(DataDir::default().cache_dir(), Utf8PathBuf::from("/var/tedge/cache"));
    /// ```
    pub fn cache_dir(&self) -> Utf8PathBuf {
        self.0.join("cache")
    }

    /// Return `Utf8PathBuf` to ThinEdge file firmware repository.
    ///
    /// # Examples
    ///
    /// ```
    /// use camino::Utf8PathBuf;
    /// use tedge_api::path::DataDir;
    ///
    /// assert_eq!(DataDir::default().firmware_dir(), Utf8PathBuf::from("/var/tedge/firmware"));
    /// ```
    pub fn firmware_dir(&self) -> Utf8PathBuf {
        self.0.join("firmware")
    }
}
