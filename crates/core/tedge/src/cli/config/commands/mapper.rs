use camino::Utf8Path;

use crate::command::Command;
use crate::log::MaybeFancy;
use tedge_config::TEdgeConfig;
use tedge_config_engine::ops::Action;

pub struct MapperGetConfigCommand {
    pub key: String,
}

#[async_trait::async_trait]
impl Command for MapperGetConfigCommand {
    fn description(&self) -> String {
        format!("get the configuration value for key: '{}'", self.key)
    }

    async fn execute(&self, tedge_config: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        let fed = tedge_mapper_config::load_federated_config(tedge_config.root_dir())
            .map_err(anyhow::Error::new)?;
        match fed.read(&self.key).map_err(anyhow::Error::new)? {
            Some(value) => println!("{value}"),
            None => {
                eprintln!("The provided config key: '{}' is not set", self.key);
                std::process::exit(1);
            }
        }
        Ok(())
    }
}

pub struct MapperMutateConfigCommand {
    pub key: String,
    pub action: Action,
}

impl MapperMutateConfigCommand {
    pub fn set(key: String, value: String) -> Self {
        Self {
            key,
            action: Action::Set(value),
        }
    }

    pub fn unset(key: String) -> Self {
        Self {
            key,
            action: Action::Unset,
        }
    }

    pub fn add(key: String, value: String) -> Self {
        Self {
            key,
            action: Action::Add(value),
        }
    }

    pub fn remove(key: String, value: String) -> Self {
        Self {
            key,
            action: Action::Remove(value),
        }
    }
}

#[async_trait::async_trait]
impl Command for MapperMutateConfigCommand {
    fn description(&self) -> String {
        match &self.action {
            Action::Set(v) => format!("set '{}' to '{v}'", self.key),
            Action::Unset => format!("unset '{}'", self.key),
            Action::Add(v) => format!("add '{v}' to '{}'", self.key),
            Action::Remove(v) => format!("remove '{v}' from '{}'", self.key),
        }
    }

    async fn execute(&self, tedge_config: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        let config_dir = tedge_config.root_dir();
        ensure_builtin_mapper_dir(&self.key, config_dir)?;
        let mut fed =
            tedge_mapper_config::load_federated_config(config_dir).map_err(anyhow::Error::new)?;
        fed.mutate(&self.key, self.action.clone())
            .map_err(anyhow::Error::new)?;
        Ok(())
    }
}

fn ensure_builtin_mapper_dir(
    key: &str,
    config_dir: &Utf8Path,
) -> Result<(), MaybeFancy<anyhow::Error>> {
    if let Some(name) = tedge_mapper_config::extract_builtin_mapper_name(key, config_dir.as_std_path()) {
        let dir = config_dir.join("mappers").join(&name);
        if !dir.exists() {
            std::fs::create_dir_all(&dir)
                .map_err(|e| anyhow::anyhow!("creating mapper directory {dir}: {e}"))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_base_mapper_directory() {
        let dir = tempfile::tempdir().unwrap();
        let config_dir = Utf8Path::from_path(dir.path()).unwrap();

        ensure_builtin_mapper_dir("mappers.c8y.url", config_dir).unwrap();

        assert!(config_dir.join("mappers/c8y").is_dir());
    }

    #[test]
    fn creates_profiled_mapper_directory() {
        let dir = tempfile::tempdir().unwrap();
        let config_dir = Utf8Path::from_path(dir.path()).unwrap();

        ensure_builtin_mapper_dir("mappers.c8y.prod.url", config_dir).unwrap();

        assert!(
            config_dir.join("mappers/c8y.prod").is_dir(),
            "should create mappers/c8y.prod/, not mappers/c8y/"
        );
        assert!(
            !config_dir.join("mappers/c8y").exists(),
            "should not create mappers/c8y/ for a profiled key"
        );
    }

    #[test]
    fn skips_non_builtin_mapper() {
        let dir = tempfile::tempdir().unwrap();
        let config_dir = Utf8Path::from_path(dir.path()).unwrap();

        ensure_builtin_mapper_dir("mappers.thingsboard.url", config_dir).unwrap();

        assert!(!config_dir.join("mappers/thingsboard").exists());
    }

    #[test]
    fn skips_non_mapper_key() {
        let dir = tempfile::tempdir().unwrap();
        let config_dir = Utf8Path::from_path(dir.path()).unwrap();

        ensure_builtin_mapper_dir("device.id", config_dir).unwrap();

        assert!(!config_dir.join("mappers").exists());
    }
}
