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

/// Resolves a relative path to an absolute path as a Utf8PathBuf.
/// The path does not need to exist on the file system.
/// This can be removed once MSRV is updated to >= 1.79 as the 'absolute_utf8'
/// function of camino can be used
///
/// # Arguments
///
/// * `relative_path` - A path-like object representing the relative path to resolve
///
/// # Returns
///
/// * `Result<Utf8PathBuf, io::Error>` - The absolute path as Utf8PathBuf or an error
///
/// # Examples
///
/// ```
/// let abs_path = resolve_to_absolute_utf8_path("../some/file.txt")?;
/// println!("Absolute path: {}", abs_path);
/// ```
pub fn resolve_to_absolute_utf8_path<P: AsRef<Utf8Path>>(
    relative_path: P,
) -> Result<Utf8PathBuf, std::io::Error> {
    let path = relative_path.as_ref();

    // If the path is already absolute, return it
    if path.is_absolute() {
        return Ok(path.to_owned());
    }

    // Get the current directory and convert to Utf8PathBuf
    let current_dir = std::env::current_dir()?;
    let current_dir = match Utf8PathBuf::from_path_buf(current_dir) {
        Ok(dir) => dir,
        Err(non_utf8_path) => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "Current directory path is not valid UTF-8: {:?}",
                    non_utf8_path
                ),
            ));
        }
    };

    // Join with the relative path and canonicalize virtually
    let joined_path = current_dir.join(path);
    let absolute_path = joined_path.canonicalize_virtually();

    Ok(absolute_path)
}

/// Extension trait for Utf8PathBuf that provides a way to canonicalize a path
/// without requiring the path to exist on the file system.
trait VirtualCanonicalize {
    fn canonicalize_virtually(&self) -> Utf8PathBuf;
}

impl VirtualCanonicalize for Utf8PathBuf {
    fn canonicalize_virtually(&self) -> Utf8PathBuf {
        let mut result = Utf8PathBuf::new();
        let is_absolute = self.is_absolute();

        if is_absolute {
            // For Windows, keep the drive or UNC prefix
            #[cfg(windows)]
            if let Some(prefix) = self.components().next() {
                // Convert the prefix component to a string
                let prefix_str = prefix.as_os_str().to_string_lossy();
                result.push(prefix_str.as_ref());
            }

            // For Unix, start with root
            #[cfg(unix)]
            result.push("/");
        }

        for component in self.components() {
            match component {
                camino::Utf8Component::Prefix(_) if !is_absolute => {
                    let comp_str = component.as_str();
                    result.push(comp_str);
                }
                camino::Utf8Component::RootDir if !is_absolute => {
                    let comp_str = component.as_str();
                    result.push(comp_str);
                }
                camino::Utf8Component::CurDir => {} // Skip current directory components (.)
                camino::Utf8Component::ParentDir => {
                    // Handle parent directory (..)
                    result.pop();
                }
                camino::Utf8Component::Normal(name) => {
                    // Add normal components
                    result.push(name);
                }
                _ => {}
            }
        }

        result
    }
}

/// Helper function to convert a standard Path to a Utf8Path
pub fn to_utf8_path<P: AsRef<Path>>(path: P) -> Result<Utf8PathBuf, std::io::Error> {
    match Utf8PathBuf::from_path_buf(path.as_ref().to_path_buf()) {
        Ok(utf8_path) => Ok(utf8_path),
        Err(non_utf8_path) => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Path is not valid UTF-8: {:?}", non_utf8_path),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_already_absolute() {
        #[cfg(windows)]
        let abs_path = "C:\\path\\to\\file.txt";
        #[cfg(unix)]
        let abs_path = "/path/to/file.txt";

        let result = resolve_to_absolute_utf8_path(abs_path).unwrap();
        assert_eq!(result, abs_path);
    }

    #[test]
    fn test_resolve_relative() {
        // This test is harder to write in a platform-independent way
        // since it depends on the current directory
        let result = resolve_to_absolute_utf8_path("./test.txt").unwrap();
        assert!(result.is_absolute());
        assert!(result.ends_with("test.txt"));
    }

    #[test]
    fn test_parent_navigation() {
        let result = resolve_to_absolute_utf8_path("../parent_file.txt").unwrap();
        assert!(result.is_absolute());
        assert!(result.ends_with("parent_file.txt"));
    }
}
