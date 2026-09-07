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
        let mut fed = tedge_mapper_config::load_federated_config(tedge_config.root_dir())
            .map_err(anyhow::Error::new)?;
        fed.mutate(&self.key, self.action.clone())
            .map_err(anyhow::Error::new)?;
        Ok(())
    }
}
