use camino::Utf8Path;
use camino::Utf8PathBuf;

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
        mutate_mapper_key(tedge_config.root_dir(), &self.key, self.action.clone())
    }
}

fn mutate_mapper_key(
    config_dir: &Utf8Path,
    key: &str,
    action: Action,
) -> Result<(), MaybeFancy<anyhow::Error>> {
    // The directory has to exist before loading, as only existing directories are mounted
    let created = ensure_builtin_mapper_dir(key, config_dir)?;
    let result = tedge_mapper_config::load_federated_config(config_dir)
        .and_then(|mut fed| fed.mutate(key, action));
    if let (Err(_), Some(created)) = (&result, created) {
        // This command created the directory and its write failed, so it holds nothing to keep
        if let Err(e) = std::fs::remove_dir_all(&created) {
            tracing::warn!("Failed to remove mapper directory {created}: {e}");
        }
    }
    result.map_err(|e| anyhow::Error::new(e).into())
}

/// Creates the directory for a built-in mapper key if it doesn't exist yet,
/// returning the outermost directory it created
fn ensure_builtin_mapper_dir(
    key: &str,
    config_dir: &Utf8Path,
) -> Result<Option<Utf8PathBuf>, MaybeFancy<anyhow::Error>> {
    let Some(name) = tedge_mapper_config::extract_builtin_mapper_name(key) else {
        return Ok(None);
    };
    let mappers = config_dir.join("mappers");
    let dir = mappers.join(&name);
    let created = if !mappers.exists() {
        mappers
    } else if !dir.exists() {
        dir.clone()
    } else {
        return Ok(None);
    };
    std::fs::create_dir_all(&dir)
        .map_err(|e| anyhow::anyhow!("creating mapper directory {dir}: {e}"))?;
    Ok(Some(created))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tedge_test_utils::fs::TempTedgeDir;

    #[rstest::rstest]
    #[case::unknown_key("mappers.c8y.prod.url", "c8y.example.com:8883")]
    #[case::invalid_value("mappers.c8y.bridge.tls.enable", "on")]
    #[case::invalid_profile_value("mappers.c8y.profiles.prod.bridge.tls.enable", "on")]
    fn a_rejected_write_creates_no_mapper_directory(#[case] key: &str, #[case] value: &str) {
        let dir = TempTedgeDir::new();

        mutate_mapper_key(dir.path(), key, Action::Set(value.into())).unwrap_err();

        assert!(
            !dir.path().join("mappers").exists(),
            "a failed write should leave no mappers directory behind"
        );
    }

    #[test]
    fn a_rejected_write_keeps_an_existing_mapper_directory() {
        let dir = TempTedgeDir::new();
        dir.dir("mappers").dir("c8y");

        mutate_mapper_key(
            dir.path(),
            "mappers.c8y.bridge.tls.enable",
            Action::Set("on".into()),
        )
        .unwrap_err();

        assert!(dir.path().join("mappers/c8y").is_dir());
    }

    #[test]
    fn a_write_creates_the_mapper_file() {
        let dir = TempTedgeDir::new();

        mutate_mapper_key(
            dir.path(),
            "mappers.c8y.url",
            Action::Set("c8y.example.com:8883".into()),
        )
        .unwrap();

        assert!(dir.path().join("mappers/c8y/mapper.toml").is_file());
    }

    #[test]
    fn creates_base_mapper_directory() {
        let dir = TempTedgeDir::new();
        let config_dir = dir.path();

        ensure_builtin_mapper_dir("mappers.c8y.url", config_dir).unwrap();

        assert!(config_dir.join("mappers/c8y").is_dir());
    }

    #[test]
    fn creates_profiled_mapper_directory() {
        let dir = TempTedgeDir::new();
        let config_dir = dir.path();

        ensure_builtin_mapper_dir("mappers.c8y.profiles.prod.url", config_dir).unwrap();

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
    fn key_without_profiles_creates_no_profile_directory() {
        let dir = TempTedgeDir::new();
        let config_dir = dir.path();

        ensure_builtin_mapper_dir("mappers.c8y.prod.url", config_dir).unwrap();

        assert!(!config_dir.join("mappers/c8y.prod").exists());
    }

    #[test]
    fn skips_non_builtin_mapper() {
        let dir = TempTedgeDir::new();
        let config_dir = dir.path();

        ensure_builtin_mapper_dir("mappers.thingsboard.url", config_dir).unwrap();

        assert!(!config_dir.join("mappers/thingsboard").exists());
    }

    #[test]
    fn skips_non_mapper_key() {
        let dir = TempTedgeDir::new();
        let config_dir = dir.path();

        ensure_builtin_mapper_dir("device.id", config_dir).unwrap();

        assert!(!config_dir.join("mappers").exists());
    }
}
