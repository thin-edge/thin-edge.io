use camino::Utf8Path;
use camino::Utf8PathBuf;
use std::fs;
use std::fs::OpenOptions;
use std::fs::Permissions;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::prelude::OpenOptionsExt;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;
#[derive(Debug, Clone)]
pub struct TempTedgeDir {
    pub temp_dir: Arc<TempDir>,
    current_file_path: Utf8PathBuf,
}

#[derive(Debug, Clone)]
pub struct TempTedgeFile {
    pub file_path: PathBuf,
}

impl Default for TempTedgeDir {
    fn default() -> Self {
        let temp_dir = if let Ok(dir) = std::env::var("TEMP_DIR_ROOT") {
            TempDir::new_in(dir).unwrap()
        } else {
            TempDir::new().unwrap()
        };
        let current_file_path = Utf8Path::from_path(temp_dir.path()).unwrap().to_owned();
        TempTedgeDir {
            temp_dir: Arc::new(temp_dir),
            current_file_path,
        }
    }
}

impl TempTedgeDir {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn dir(&self, directory_name: &str) -> TempTedgeDir {
        let root = Utf8Path::from_path(self.temp_dir.path()).unwrap();
        let path = root.join(&self.current_file_path).join(directory_name);

        if !path.exists() {
            fs::create_dir(&path).unwrap();
        };

        TempTedgeDir {
            temp_dir: self.temp_dir.clone(),
            current_file_path: path,
        }
    }

    pub fn file(&self, file_name: &str) -> TempTedgeFile {
        let root = self.temp_dir.path().to_path_buf();
        let path = root.join(&self.current_file_path).join(file_name);

        if !path.exists() {
            let file = fs::File::create(&path).unwrap();
            file.sync_all().unwrap();
        };
        TempTedgeFile { file_path: path }
    }

    pub fn link_dir(&self, original: &TempTedgeDir, link_name: &str) -> TempTedgeDir {
        let root = Utf8Path::from_path(self.temp_dir.path()).unwrap();
        let path = root.join(&self.current_file_path).join(link_name);

        #[cfg(unix)]
        std::os::unix::fs::symlink(original.path(), &path).unwrap();

        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(original.path(), &path).unwrap();

        TempTedgeDir {
            temp_dir: self.temp_dir.clone(),
            current_file_path: path,
        }
    }

    pub fn path(&self) -> &Path {
        self.current_file_path.as_std_path()
    }

    pub fn utf8_path(&self) -> &Utf8Path {
        self.current_file_path.as_path()
    }

    pub fn utf8_path_buf(&self) -> Utf8PathBuf {
        self.current_file_path.clone()
    }

    pub fn to_path_buf(&self) -> PathBuf {
        self.current_file_path.clone().into_std_path_buf()
    }

    pub fn set_mode(&self, mode: u32) {
        std::fs::set_permissions(&self.current_file_path, Permissions::from_mode(mode)).unwrap();
    }
}

impl TempTedgeFile {
    pub fn with_raw_content(self, content: &str) -> Self {
        let mut file = OpenOptions::new()
            .write(true)
            .create(false)
            .open(self.file_path.clone())
            .unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file.flush().unwrap();
        file.sync_all().unwrap();
        self
    }

    pub fn with_toml_content(self, content: toml::Table) {
        let mut file = OpenOptions::new()
            .write(true)
            .create(false)
            .open(self.file_path)
            .unwrap();
        let file_content = content.to_string();
        file.write_all(file_content.as_bytes()).unwrap();
        file.flush().unwrap();
        file.sync_all().unwrap();
    }

    pub fn delete(self) {
        std::fs::remove_file(self.path()).unwrap();
    }

    pub fn path(&self) -> &Path {
        Path::new(&self.file_path)
    }

    pub fn utf8_path(&self) -> &Utf8Path {
        Utf8Path::from_path(self.path()).unwrap()
    }

    pub fn to_path_buf(&self) -> PathBuf {
        PathBuf::from(self.path())
    }

    pub fn utf8_path_buf(&self) -> Utf8PathBuf {
        Utf8PathBuf::from_path_buf(self.to_path_buf()).unwrap()
    }
}

pub fn with_exec_permission(file_path: impl AsRef<Path>, content: &str) {
    let mut file = OpenOptions::new()
        .mode(0o744)
        .create_new(true)
        .write(true)
        .open(file_path)
        .unwrap();

    file.write_all(content.as_bytes()).unwrap();
    file.flush().unwrap();
    file.sync_all().unwrap();
}

pub fn create_full_tedge_dir_structure() {
    let ttd = TempTedgeDir::new();
    ttd.file("tedge.toml");
    ttd.dir("c8y")
        .file("c8y-log-plugin.toml")
        .with_toml_content(toml::toml! {
            files = [
                {type = "software-management", path = "/var/log/tedge/agent/workflow-software_*" }
            ]
        });
    ttd.dir("contrib").dir("collectd").file("collectd.conf");
    ttd.dir("device").file("inventory.json");
    ttd.dir("device-certs");
    ttd.dir("mosquitto-conf").file("c8y-bridge.conf");
    ttd.dir("mosquitto-conf").file("tedge-mosquitto.conf");
    ttd.dir("operations")
        .dir("c8y")
        .file("c8y_LogfileRequest")
        .with_raw_content("");
    ttd.dir("operations").dir("c8y").file("c8y_Restart");
    ttd.dir("operations").dir("c8y").file("c8y_SoftwareUpdate");
    ttd.dir("sm-plugins").file("apt");
}

#[cfg(test)]
mod tests {
    use super::TempTedgeDir;
    use std::io::Read;
    use std::path::Path;

    #[test]
    fn assert_dir_file_and_content() -> Result<(), anyhow::Error> {
        let tedge_dir = TempTedgeDir::new();
        tedge_dir.dir("c8y").file("c8y-log-plugin.toml");
        tedge_dir
            .dir("operations")
            .dir("c8y")
            .file("c8y_Restart")
            .with_toml_content(toml::toml! {
                files = []
            });

        assert!(Path::new(&format!(
            "{}/c8y/c8y-log-plugin.toml",
            &tedge_dir.temp_dir.path().to_str().unwrap()
        ))
        .exists());

        assert!(Path::new(&format!(
            "{}/operations/c8y/c8y_Restart",
            &tedge_dir.temp_dir.path().to_str().unwrap()
        ))
        .exists());
        Ok(())
    }

    #[test]
    fn test_with_toml() -> Result<(), anyhow::Error> {
        let tedge_dir = TempTedgeDir::new();
        tedge_dir
            .dir("c8y")
            .file("c8y-log-plugin.toml")
            .with_toml_content(toml::toml! {
                files = [
                    { type = "apt", path = "/var/log/apt/history.log"}
                ]
            });
        let file_path = &format!(
            "{}/c8y/c8y-log-plugin.toml",
            &tedge_dir.temp_dir.path().to_str().unwrap()
        );
        assert!(Path::new(&file_path).exists());

        let mut file_content = String::new();
        let mut file = std::fs::File::open(file_path).unwrap();
        file.read_to_string(&mut file_content).unwrap();

        let as_toml: toml::Value = toml::from_str(&file_content).unwrap();
        assert_eq!(
            as_toml,
            toml::Value::Table(toml::toml! {
                files = [
                    { type = "apt", path = "/var/log/apt/history.log"}
                ]
            })
        );

        Ok(())
    }

    #[test]
    fn test_multiple_files_in_same_dir() -> Result<(), anyhow::Error> {
        let ttd = TempTedgeDir::new();
        let operations_dir = ttd.dir("operations");
        operations_dir.dir("c8y").file("c8y_Restart");
        operations_dir.dir("c8y").file("c8y_SoftwareUpdate");

        assert!(Path::new(&format!(
            "{}/operations/c8y/c8y_Restart",
            &ttd.temp_dir.path().to_str().unwrap()
        ))
        .exists());

        assert!(Path::new(&format!(
            "{}/operations/c8y/c8y_SoftwareUpdate",
            &ttd.temp_dir.path().to_str().unwrap()
        ))
        .exists());
        Ok(())
    }
}
