use crate::error::StateError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, str::FromStr};
use tedge_config::TEdgeConfigLocation;
use tedge_utils::fs::atomically_write_file_async;

use tokio::fs;

#[derive(Debug)]
pub struct AgentStateRepository {
    state_repo_path: PathBuf,
    state_repo_root: PathBuf,
}

#[async_trait]
pub trait StateRepository<T> {
    type Error;
    async fn load(&self) -> Result<T, Self::Error>;
    async fn store(&self, state: &T) -> Result<(), Self::Error>;
    async fn clear(&self) -> Result<T, Self::Error>;
}

#[async_trait]
impl StateRepository<State> for AgentStateRepository {
    type Error = StateError;

    async fn load(&self) -> Result<State, StateError> {
        match fs::read(&self.state_repo_path).await {
            Ok(bytes) => Ok(toml::from_slice::<State>(bytes.as_slice())?),

            Err(err) => Err(StateError::IOError(err)),
        }
    }

    async fn store(&self, state: &State) -> Result<(), StateError> {
        let toml = toml::to_string_pretty(&state)?;

        // Create `$HOME/.tedge` or `/etc/tedge` directory in case it does not exist yet
        if !self.state_repo_root.exists() {
            let () = fs::create_dir(&self.state_repo_root).await?;
        }

        let mut temppath = self.state_repo_path.clone();
        temppath.set_extension("tmp");

        let () =
            atomically_write_file_async(temppath, &self.state_repo_path, toml.as_bytes()).await?;

        Ok(())
    }

    async fn clear(&self) -> Result<State, Self::Error> {
        let state = State {
            operation_id: None,
            operation: None,
        };
        let () = self.store(&state).await?;

        Ok(state)
    }
}

impl AgentStateRepository {
    pub fn new(config_location: &TEdgeConfigLocation) -> Self {
        let mut state_repo_root = config_location.tedge_config_root_path.clone();
        state_repo_root.push(PathBuf::from_str(".agent").expect("infallible"));

        let mut state_repo_path = state_repo_root.clone();
        state_repo_path.push(PathBuf::from_str("current-operation").expect("infallible"));

        Self {
            state_repo_path,
            state_repo_root,
        }
    }
}

#[derive(Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub operation_id: Option<usize>,
    pub operation: Option<String>,
}

#[cfg(test)]
mod tests {
    use crate::state::{AgentStateRepository, State, StateRepository};

    use tempfile::{tempdir, NamedTempFile};

    #[tokio::test]
    async fn agent_state_repository_not_exists_fail() {
        let _temp_dir = tempdir().unwrap();

        let temp_dir = tempdir().unwrap();
        let temp_config_file = NamedTempFile::new().unwrap();

        let config = tedge_config::TEdgeConfigLocation {
            tedge_config_root_path: temp_dir.into_path(),
            tedge_config_file_path: temp_config_file.path().to_owned(),
        };
        let repo = AgentStateRepository::new(&config);

        repo.load().await.unwrap_err();
    }

    #[tokio::test]
    async fn agent_state_repository_exists_loads_some() {
        let temp_dir = tempdir().unwrap();

        let _ = tokio::fs::create_dir(temp_dir.path().join(".agent/")).await;
        let destination_path = temp_dir.path().join(".agent/current-operation");

        let content = "operation_id = 1234\noperation = \"list\"";

        let _ = tokio::fs::write(destination_path, content.as_bytes()).await;

        let temp_config_file = NamedTempFile::new().unwrap();
        let config = tedge_config::TEdgeConfigLocation {
            tedge_config_root_path: temp_dir.into_path(),
            tedge_config_file_path: temp_config_file.path().to_owned(),
        };

        let repo = AgentStateRepository::new(&config);

        let data = repo.load().await.unwrap();
        assert_eq!(
            data,
            State {
                operation_id: Some(1234),
                operation: Some("list".into()),
            }
        );
    }

    #[tokio::test]
    async fn agent_state_repository_exists_loads_none() {
        let temp_dir = tempdir().unwrap();

        let _ = tokio::fs::create_dir(temp_dir.path().join(".agent/")).await;
        let destination_path = temp_dir.path().join(".agent/current-operation");

        let content = "";

        let _ = tokio::fs::write(destination_path, content.as_bytes()).await;

        let temp_config_file = NamedTempFile::new().unwrap();
        let config = tedge_config::TEdgeConfigLocation {
            tedge_config_root_path: temp_dir.into_path(),
            tedge_config_file_path: temp_config_file.path().to_owned(),
        };

        let repo = AgentStateRepository::new(&config);

        let data = repo.load().await.unwrap();
        assert_eq!(
            data,
            State {
                operation_id: None,
                operation: None
            }
        );
    }

    #[tokio::test]
    async fn agent_state_repository_exists_store() {
        let temp_dir = tempdir().unwrap();
        let temp_config_file = NamedTempFile::new().unwrap();

        let _ = tokio::fs::create_dir(temp_dir.path().join(".agent/")).await;
        let destination_path = temp_dir.path().join(".agent/current-operation");

        let config = tedge_config::TEdgeConfigLocation {
            tedge_config_root_path: temp_dir.into_path(),
            tedge_config_file_path: temp_config_file.path().to_owned(),
        };

        let content = "operation_id = 1234";

        let _ = tokio::fs::write(&destination_path, content.as_bytes()).await;

        let repo = AgentStateRepository::new(&config);

        repo.store(&State {
            operation_id: Some(1234),
            operation: Some("list".into()),
        })
        .await
        .unwrap();

        let data = tokio::fs::read_to_string(destination_path).await.unwrap();

        assert_eq!(data, "operation_id = 1234\noperation = \'list\'\n");
    }
}
