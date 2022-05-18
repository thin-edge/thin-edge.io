use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
};
use tempfile::TempDir;

struct TempTedgeDir {
    temp_dir: TempDir,
}

struct TedgeChildTempDir {
    child_temp_dir: PathBuf,
}

struct TedgeTempFile {
    file_path: PathBuf,
}

impl TempTedgeDir {
    pub fn new() -> Self {
        let temp_dir = TempDir::new().unwrap();
        TempTedgeDir { temp_dir }
    }

    pub fn dir(&self, directory_name: &str) -> TedgeChildTempDir {
        let root = self.temp_dir.path().to_path_buf();
        let path = root.join(directory_name);

        if !path.exists() {
            let () = fs::create_dir(&path).unwrap();
        };

        TedgeChildTempDir {
            child_temp_dir: path,
        }
    }

    pub fn file(&self, file_name: &str) -> TedgeTempFile {
        let root = self.temp_dir.path().to_path_buf();
        let path = root.join(file_name);

        if !path.exists() {
            let _file = fs::File::create(&path).unwrap();
        };
        TedgeTempFile { file_path: path }
    }
}

impl TedgeChildTempDir {
    fn file(&self, file_name: &str) -> TedgeTempFile {
        let root = self.child_temp_dir.to_path_buf();
        let path = root.join(file_name);

        if !path.exists() {
            let _file = fs::File::create(&path).unwrap();
        };
        TedgeTempFile { file_path: path }
    }

    fn dir(&self, directory_name: &str) -> Self {
        let root = self.child_temp_dir.to_path_buf();
        let path = root.join(directory_name);

        if !path.exists() {
            let () = fs::create_dir(&path).unwrap();
        };

        TedgeChildTempDir {
            child_temp_dir: path,
        }
    }
}

impl TedgeTempFile {
    fn with_raw_content(self, content: &str) {
        let mut file = OpenOptions::new()
            .write(true)
            .create(false)
            .open(self.file_path)
            .unwrap();
        file.write_all(content.as_bytes()).unwrap();
    }

    fn with_toml_content(self, content: toml::Value) {
        let mut file = OpenOptions::new()
            .write(true)
            .create(false)
            .open(self.file_path)
            .unwrap();
        let file_content = content.to_string();
        file.write_all(file_content.as_bytes()).unwrap();
    }
}

pub fn create_full_tedge_dir_structure() {
    let ttd = TempTedgeDir::new();
    ttd.file("tedge.toml");
    ttd.dir(".agent").file("current-operation");
    ttd.dir("c8y")
        .file("c8y-log-plugin.toml")
        .with_toml_content(toml::toml! {
            files = [
                {type = "software-management", path = "/var/log/tedge/agent/software-*" }
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
    use std::{io::Read, path::Path};

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
            .file("c8y_log_plugin.toml")
            .with_toml_content(toml::toml! {
                files = [
                    { type = "apt", path = "/var/log/apt/history.log"}
                ]
            });
        let file_path = &format!(
            "{}/c8y/c8y_log_plugin.toml",
            &tedge_dir.temp_dir.path().to_str().unwrap()
        );
        assert!(Path::new(&file_path).exists());

        let mut file_content = String::new();
        let mut file = std::fs::File::open(&file_path).unwrap();
        file.read_to_string(&mut file_content).unwrap();

        let as_toml: toml::Value = toml::from_str(&file_content).unwrap();
        assert_eq!(
            as_toml,
            toml::toml! {
                files = [
                    { type = "apt", path = "/var/log/apt/history.log"}
                ]
            }
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
