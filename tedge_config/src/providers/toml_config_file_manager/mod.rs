use crate::*;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fs::create_dir_all;
use std::io::Write;
use std::path::PathBuf;
use tempfile::NamedTempFile;

pub struct TomlConfigFileManager<T> {
    path: PathBuf,
    data: T,
    dirty: bool,
}

impl<T> TomlConfigFileManager<T>
where
    T: DeserializeOwned,
{
    pub fn from_file(path: PathBuf) -> Result<Self, ConfigError> {
        match std::fs::read(&path) {
            Ok(bytes) => {
                let data = toml::from_slice::<T>(bytes.as_slice())?;
                Ok(Self {
                    path,
                    data,
                    dirty: false,
                })
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                Err(ConfigError::ConfigFileNotFound(path))
            }
            Err(err) => Err(ConfigError::IOError(err)),
        }
    }
}

impl<T> TomlConfigFileManager<T>
where
    T: Serialize,
{
    pub fn persist(&mut self) -> Result<(), ConfigError> {
        let toml = toml::to_string_pretty(&self.data)?;
        let mut file = NamedTempFile::new()?;
        file.write_all(toml.as_bytes())?;
        if !self.path.exists() {
            create_dir_all(self.path.parent().unwrap())?;
        }
        match file.persist(&self.path) {
            Ok(_) => Ok(()),
            Err(err) => Err(err.error.into()),
        }
    }
}

impl<T> TomlConfigFileManager<T> {
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }
}

impl<T, S: ConfigSetting> QuerySetting<S> for TomlConfigFileManager<T>
where
    T: QuerySetting<S>,
{
    fn query(&self, setting: S) -> ConfigSettingResult<S::Value> {
        self.data.query(setting)
    }
}

impl<T, S: ConfigSetting> UpdateSetting<S> for TomlConfigFileManager<T>
where
    T: UpdateSetting<S>,
{
    fn update(&mut self, setting: S, value: S::Value) -> ConfigSettingResult<()> {
        self.mark_dirty();
        self.data.update(setting, value)
    }
}

impl<T, S: ConfigSetting> UnsetSetting<S> for TomlConfigFileManager<T>
where
    T: UnsetSetting<S>,
{
    fn unset(&mut self, setting: S) -> ConfigSettingResult<()> {
        self.mark_dirty();
        self.data.unset(setting)
    }
}
