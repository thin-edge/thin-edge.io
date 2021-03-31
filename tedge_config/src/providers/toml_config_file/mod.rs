use crate::*;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fs::create_dir_all;
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

#[derive(Debug)]
pub struct TomlConfigFile<T> {
    path: PathBuf,
    data: T,
    dirty: bool,
}

impl<T> TomlConfigFile<T>
where
    T: DeserializeOwned,
{
    pub fn from_file(path: PathBuf) -> Result<Self, TEdgeConfigError> {
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
                Err(TEdgeConfigError::ConfigFileNotFound(path))
            }
            Err(err) => Err(TEdgeConfigError::IOError(err)),
        }
    }
}

impl<T> TomlConfigFile<T>
where
    T: DeserializeOwned + Default,
{
    pub fn from_file_or_default(path: PathBuf) -> Result<Self, TEdgeConfigError> {
        match Self::from_file(path.clone()) {
            Ok(file) => Ok(file),
            Err(TEdgeConfigError::ConfigFileNotFound(..)) => Ok(Self {
                path,
                data: T::default(),
                dirty: true,
            }),
            Err(err) => Err(err),
        }
    }
}

impl<T> TomlConfigFile<T>
where
    T: Serialize,
{
    pub fn persist(&mut self) -> Result<(), TEdgeConfigError> {
        self.write_to(&self.path)?;
        self.undirty();
        Ok(())
    }

    fn write_to(&self, path: &Path) -> Result<(), TEdgeConfigError> {
        let toml = toml::to_string_pretty(&self.data)?;
        let mut file = NamedTempFile::new()?;
        file.write_all(toml.as_bytes())?;
        if !self.path.exists() {
            create_dir_all(self.path.parent().unwrap())?;
        }
        match file.persist(path) {
            Ok(_) => Ok(()),
            Err(err) => Err(err.error.into()),
        }
    }
}

impl<T> TomlConfigFile<T> {
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    fn undirty(&mut self) {
        self.dirty = false;
    }
}

impl<T, S: ConfigSetting> QuerySetting<S> for TomlConfigFile<T>
where
    T: QuerySetting<S>,
{
    fn query(&self, setting: S) -> ConfigSettingResult<S::Value> {
        self.data.query(setting)
    }
}

impl<T, S: ConfigSetting> UpdateSetting<S> for TomlConfigFile<T>
where
    T: UpdateSetting<S>,
{
    fn update(&mut self, setting: S, value: S::Value) -> ConfigSettingResult<()> {
        self.mark_dirty();
        self.data.update(setting, value)
    }
}

impl<T, S: ConfigSetting> UnsetSetting<S> for TomlConfigFile<T>
where
    T: UnsetSetting<S>,
{
    fn unset(&mut self, setting: S) -> ConfigSettingResult<()> {
        self.mark_dirty();
        self.data.unset(setting)
    }
}
