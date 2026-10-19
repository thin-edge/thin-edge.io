use crate::command::Command;
use crate::log::MaybeFancy;
use tedge_config::TEdgeConfig;
use tedge_config_engine::ops::Action;
use tedge_utils::paths::TedgePaths;

pub struct MapperGetConfigCommand {
    pub key: String,
}

#[async_trait::async_trait]
impl Command for MapperGetConfigCommand {
    fn description(&self) -> String {
        format!("get the configuration value for key: '{}'", self.key)
    }

    async fn execute(&self, tedge_config: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        let fed = tedge_mapper_config::load_federated_config(&tedge_config.config_root())
            .await
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
        mutate_mapper_key(&tedge_config.config_root(), &self.key, self.action.clone())
            .await
            .map_err(|e| anyhow::Error::new(e).into())
    }
}

async fn mutate_mapper_key(
    config_root: &TedgePaths,
    key: &str,
    action: Action,
) -> Result<(), tedge_config_engine::ConfigError> {
    let mut fed = tedge_mapper_config::load_federated_config(config_root).await?;
    tedge_mapper_config::mount_builtin_mapper(&mut fed, config_root, key)?;
    fed.mutate(key, action).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tedge_test_utils::fs::TempTedgeDir;

    #[rstest::rstest]
    #[case::unknown_key("mappers.c8y.prod.url", "c8y.example.com:8883")]
    #[case::unknown_mapper("mappers.thingsboard.url", "tb.example.com:8883")]
    #[case::invalid_value("mappers.c8y.bridge.tls.enable", "on")]
    #[case::invalid_profile_value("mappers.c8y.profiles.prod.bridge.tls.enable", "on")]
    #[tokio::test]
    async fn a_rejected_write_creates_no_mapper_directory(#[case] key: &str, #[case] value: &str) {
        let dir = TempTedgeDir::new();

        mutate_mapper_key(&config_root(&dir), key, Action::Set(value.into()))
            .await
            .unwrap_err();

        assert!(
            !dir.path().join("mappers").exists(),
            "a failed write should leave no mappers directory behind"
        );
    }

    #[tokio::test]
    async fn a_rejected_write_keeps_an_existing_mapper_directory() {
        let dir = TempTedgeDir::new();
        dir.dir("mappers").dir("c8y");

        mutate_mapper_key(
            &config_root(&dir),
            "mappers.c8y.bridge.tls.enable",
            Action::Set("on".into()),
        )
        .await
        .unwrap_err();

        assert!(dir.path().join("mappers/c8y").is_dir());
    }

    #[tokio::test]
    async fn a_write_creates_the_mapper_file() {
        let dir = TempTedgeDir::new();

        mutate_mapper_key(
            &config_root(&dir),
            "mappers.c8y.url",
            Action::Set("c8y.example.com:8883".into()),
        )
        .await
        .unwrap();

        assert!(dir.path().join("mappers/c8y/mapper.toml").is_file());
    }

    #[tokio::test]
    async fn a_write_to_a_profile_creates_only_the_profiled_mapper_file() {
        let dir = TempTedgeDir::new();

        mutate_mapper_key(
            &config_root(&dir),
            "mappers.c8y.profiles.prod.url",
            Action::Set("c8y.example.com:8883".into()),
        )
        .await
        .unwrap();

        assert!(dir.path().join("mappers/c8y.prod/mapper.toml").is_file());
        assert!(!dir.path().join("mappers/c8y").exists());
    }

    fn config_root(dir: &TempTedgeDir) -> TedgePaths {
        TedgePaths::from_root_with_defaults(dir.path(), "", "")
    }
}
