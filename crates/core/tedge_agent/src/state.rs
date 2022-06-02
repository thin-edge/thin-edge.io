use crate::error::StateError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, str::FromStr};
use tedge_utils::fs::atomically_write_file_async;
use tokio::fs;
use tracing::error;

#[derive(Debug)]
pub struct AgentStateRepository {
    state_repo_path: PathBuf,
    state_repo_root: PathBuf,
}

#[async_trait]
pub trait StateRepository {
    type Error;
    async fn load(&self) -> Result<State, Self::Error>;
    async fn store(&self, state: &State) -> Result<(), Self::Error>;
    async fn clear(&self) -> Result<State, Self::Error>;
    async fn update(&self, status: &StateStatus) -> Result<(), Self::Error>;
}

#[async_trait]
impl StateRepository for AgentStateRepository {
    type Error = StateError;

    async fn load(&self) -> Result<State, StateError> {
        match fs::read(&self.state_repo_path).await {
            Ok(bytes) => Ok(toml::from_slice::<State>(bytes.as_slice())?),

            Err(err) => {
                error!("Error reading: {:?}", &self.state_repo_path);
                Err(StateError::FromIo(err))
            }
        }
    }

    async fn store(&self, state: &State) -> Result<(), StateError> {
        let toml = toml::to_string_pretty(&state)?;

        // Create in path given through `config-dir` or `/etc/tedge` directory in case it does not exist yet
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

    async fn update(&self, status: &StateStatus) -> Result<(), Self::Error> {
        let mut state = self.load().await?;
        state.operation = Some(status.to_owned());

        self.store(&state).await?;

        Ok(())
    }
}

impl AgentStateRepository {
    pub fn new(tedge_root: PathBuf) -> Self {
        let mut state_repo_root = tedge_root;
        state_repo_root.push(PathBuf::from_str(".agent").expect("infallible"));

        let mut state_repo_path = state_repo_root.clone();
        state_repo_path.push(PathBuf::from_str("current-operation").expect("infallible"));

        Self {
            state_repo_path,
            state_repo_root,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(untagged)]
pub enum StateStatus {
    Software(SoftwareOperationVariants),
    Restart(RestartOperationStatus),
    UnknownOperation,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SoftwareOperationVariants {
    List,
    Update,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum RestartOperationStatus {
    Pending,
    Restarting,
}

#[derive(Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub operation_id: Option<String>,
    pub operation: Option<StateStatus>,
}

#[cfg(test)]
mod tests {
    use crate::state::{
        AgentStateRepository, RestartOperationStatus, SoftwareOperationVariants, State,
        StateRepository, StateStatus,
    };

    use tedge_test_utils::fs::TempTedgeDir;

    #[tokio::test]
    async fn agent_state_repository_not_exists_fail() {
        let temp_dir = TempTedgeDir::new();
        let repo = AgentStateRepository::new(temp_dir.path().to_path_buf());

        repo.load().await.unwrap_err();
    }

    #[tokio::test]
    async fn agent_state_repository_exists_loads_some() {
        let temp_dir = TempTedgeDir::new();
        let content = "operation_id = \'1234\'\noperation = \"list\"";
        temp_dir
            .dir(".agent")
            .file("current-operation")
            .with_raw_content(content);

        let repo = AgentStateRepository::new(temp_dir.path().to_path_buf());

        let data = repo.load().await.unwrap();
        assert_eq!(
            data,
            State {
                operation_id: Some("1234".into()),
                operation: Some(StateStatus::Software(SoftwareOperationVariants::List)),
            }
        );
    }

    #[tokio::test]
    async fn agent_state_repository_exists_loads_some_restart_variant() {
        let temp_dir = TempTedgeDir::new();
        let content = "operation_id = \'1234\'\noperation = \"Restarting\"";
        temp_dir
            .dir(".agent")
            .file("current-operation")
            .with_raw_content(content);

        let repo = AgentStateRepository::new(temp_dir.path().to_path_buf());

        let data = repo.load().await.unwrap();
        assert_eq!(
            data,
            State {
                operation_id: Some("1234".into()),
                operation: Some(StateStatus::Restart(RestartOperationStatus::Restarting)),
            }
        );
    }

    #[tokio::test]
    async fn agent_state_repository_exists_loads_none() {
        let temp_dir = TempTedgeDir::new();
        let content = "";
        temp_dir
            .dir(".agent")
            .file("current-operation")
            .with_raw_content(content);

        let repo = AgentStateRepository::new(temp_dir.path().to_path_buf());

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
        let temp_dir = TempTedgeDir::new();
        temp_dir.dir(".agent").file("current-operation");

        let repo = AgentStateRepository::new(temp_dir.path().to_path_buf());

        repo.store(&State {
            operation_id: Some("1234".into()),
            operation: Some(StateStatus::Software(SoftwareOperationVariants::List)),
        })
        .await
        .unwrap();

        let data = tokio::fs::read_to_string(&format!(
            "{}/.agent/current-operation",
            &temp_dir.temp_dir.path().to_str().unwrap()
        ))
        .await
        .unwrap();

        assert_eq!(data, "operation_id = \'1234\'\noperation = \'list\'\n");
    }
}
