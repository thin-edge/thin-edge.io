use std::path::{Path, PathBuf};

use crate::ops::{Action, ConfigOps};
use crate::reflect::{ConfigError, KeyEntry};

/// A config source mounted below a key prefix such as `mappers.c8y.`.
pub struct ConfigMount {
    pub prefix: String,
    pub source: Box<dyn ConfigOps>,
}

/// Routes config keys across the root config and mounted mapper configs.
pub struct FederatedConfig {
    mounts: Vec<ConfigMount>,
    config_dir: PathBuf,
}

impl FederatedConfig {
    /// Creates an empty federated config using `config_dir` for diagnostics.
    pub fn new(config_dir: &Path) -> Self {
        Self {
            mounts: Vec::new(),
            config_dir: config_dir.to_owned(),
        }
    }

    /// Adds a source under `prefix`; longer prefixes are matched first.
    pub fn mount(&mut self, prefix: &str, source: Box<dyn ConfigOps>) {
        self.mounts.push(ConfigMount {
            prefix: prefix.to_owned(),
            source,
        });
        self.mounts
            .sort_by(|a, b| b.prefix.len().cmp(&a.prefix.len()));
    }

    /// Reads values explicitly set in the mounted source.
    pub fn get(&self, full_key: &str) -> Result<Option<String>, ConfigError> {
        let (mount, local_key) = self.route(full_key)?;
        mount
            .source
            .get(&local_key)
            .map_err(|e| self.contextualize(full_key, e))
    }

    /// Reads the effective value, including defaults that may fall back to root config.
    pub fn read(&self, full_key: &str) -> Result<Option<String>, ConfigError> {
        let (mount, local_key) = self.route(full_key)?;
        let root_mount = self.root_mount();
        let resolve_root =
            |key: &str| root_mount.and_then(|m| m.source.read(key, None).ok().flatten());
        // The root config has no `from_root` defaults of its own, so it
        // never needs (and must not recurse into) a root resolver
        let resolver = if mount.prefix.is_empty() {
            None
        } else {
            Some(&resolve_root as &dyn Fn(&str) -> Option<String>)
        };
        mount
            .source
            .read(&local_key, resolver)
            .map_err(|e| self.contextualize(full_key, e))
    }

    /// Routes a write operation to the mounted source and persists that source.
    pub fn mutate(&mut self, full_key: &str, action: Action) -> Result<(), ConfigError> {
        let err_key = full_key.to_owned();
        let (mount, local_key) = self.route_mut(full_key)?;
        mount
            .source
            .mutate(&local_key, action)
            .map_err(|e| Self::contextualize_with(&err_key, e, &self.config_dir, &self.mounts))
    }

    /// Lists keys from every mounted source with each mount prefix applied.
    pub fn all_entries(&self) -> Vec<KeyEntry> {
        self.mounts
            .iter()
            .rev()
            .flat_map(|m| {
                m.source.entries().into_iter().map(|e| KeyEntry {
                    key: format!("{}{}", m.prefix, e.key),
                    doc: e.doc,
                    examples: e.examples,
                })
            })
            .collect()
    }

    /// Returns the currently mounted prefixes in routing order.
    pub fn mount_prefixes(&self) -> Vec<String> {
        self.mounts.iter().map(|m| m.prefix.clone()).collect()
    }

    fn root_mount(&self) -> Option<&ConfigMount> {
        self.mounts.iter().find(|m| m.prefix.is_empty())
    }

    fn route(&self, key: &str) -> Result<(&ConfigMount, String), ConfigError> {
        for mount in &self.mounts {
            if let Some(local) = key.strip_prefix(&mount.prefix) {
                return Ok((mount, local.to_owned()));
            }
        }
        Err(self.unknown_key_error(key))
    }

    fn route_mut(&mut self, key: &str) -> Result<(&mut ConfigMount, String), ConfigError> {
        let err = self.unknown_key_error(key);
        for mount in &mut self.mounts {
            if let Some(local) = key.strip_prefix(&mount.prefix) {
                return Ok((mount, local.to_owned()));
            }
        }
        Err(err)
    }

    fn contextualize(&self, full_key: &str, err: ConfigError) -> ConfigError {
        Self::contextualize_with(full_key, err, &self.config_dir, &self.mounts)
    }

    fn contextualize_with(
        full_key: &str,
        err: ConfigError,
        config_dir: &Path,
        mounts: &[ConfigMount],
    ) -> ConfigError {
        match err {
            ConfigError::UnknownKey(_) => make_unknown_key_error(full_key, config_dir, mounts),
            other => other,
        }
    }

    fn unknown_key_error(&self, key: &str) -> ConfigError {
        make_unknown_key_error(key, &self.config_dir, &self.mounts)
    }
}

fn make_unknown_key_error(key: &str, config_dir: &Path, _mounts: &[ConfigMount]) -> ConfigError {
    if let Some(rest) = key.strip_prefix("mappers.") {
        if let Some(dot) = rest.find('.') {
            let mapper_name = &rest[..dot];
            let mappers_dir = config_dir.join("mappers");
            let mut known = Vec::new();
            if let Ok(entries) = std::fs::read_dir(&mappers_dir) {
                for entry in entries.flatten() {
                    if entry.path().is_dir() {
                        if let Some(name) = entry.file_name().to_str() {
                            known.push(name.to_owned());
                        }
                    }
                }
            }
            known.sort();
            return ConfigError::UnknownMapper {
                name: mapper_name.to_owned(),
                mappers_dir,
                known,
            };
        }
    }
    ConfigError::UnknownKey(key.to_owned())
}
