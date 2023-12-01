use crate::state_repository::error::StateError;
use camino::Utf8PathBuf;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::marker::PhantomData;
use tedge_utils::fs::atomically_write_file_async;
use tokio::fs;

/// Store the current state of an operation
#[derive(Debug)]
pub struct AgentStateRepository<T> {
    pub state_repo_path: Utf8PathBuf,
    phantom: PhantomData<T>,
}

pub fn agent_state_dir(tedge_root: Utf8PathBuf) -> Utf8PathBuf {
    tedge_root.join(".agent")
}

impl<T: DeserializeOwned + Serialize> AgentStateRepository<T> {
    pub fn new(tedge_root: Utf8PathBuf, file_name: &str) -> Self {
        let state_repo_path = agent_state_dir(tedge_root).join(file_name);
        Self {
            state_repo_path,
            phantom: PhantomData,
        }
    }

    /// Load the latest operation, if any
    pub async fn load(&self) -> Result<Option<T>, StateError> {
        let text = fs::read_to_string(&self.state_repo_path)
            .await
            .map_err(|e| StateError::LoadingFromFileFailed {
                path: self.state_repo_path.as_path().into(),
                source: e,
            })?;

        if text.is_empty() {
            Ok(None)
        } else {
            let state = serde_json::from_str(&text).map_err(|e| StateError::InvalidJson {
                path: self.state_repo_path.as_path().into(),
                source: e,
            })?;
            Ok(Some(state))
        }
    }

    /// Store the current operation, persisting is JSON representation
    pub async fn store(&self, state: &T) -> Result<(), StateError> {
        let json = serde_json::to_string(state)?;
        atomically_write_file_async(&self.state_repo_path, json.as_bytes()).await?;
        Ok(())
    }

    /// Clear the current operation by clearing the persisted file
    pub async fn clear(&self) -> Result<(), StateError> {
        atomically_write_file_async(&self.state_repo_path, "".as_bytes()).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::state_repository::state::AgentStateRepository;
    use serde::Deserialize;
    use serde::Serialize;
    use tedge_test_utils::fs::TempTedgeDir;

    #[derive(Debug, Default, Deserialize, Eq, PartialEq, Serialize, Clone)]
    pub struct State {
        pub operation_id: String,
        pub operation: String,
    }

    #[tokio::test]
    async fn agent_state_repository_not_exists_fail() {
        let temp_dir = TempTedgeDir::new();
        let repo: AgentStateRepository<State> =
            AgentStateRepository::new(temp_dir.utf8_path_buf(), "current-operation");

        repo.load().await.unwrap_err();
    }

    #[tokio::test]
    async fn agent_state_repository_exists_loads_some() {
        let temp_dir = TempTedgeDir::new();
        let content = r#"{"operation_id":"1234","operation":"list"}"#;
        temp_dir
            .dir(".agent")
            .file("current-operation")
            .with_raw_content(content);

        let repo: AgentStateRepository<State> =
            AgentStateRepository::new(temp_dir.utf8_path_buf(), "current-operation");

        let data = repo.load().await.unwrap();
        assert_eq!(
            data,
            Some(State {
                operation_id: "1234".to_string(),
                operation: "list".to_string(),
            })
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

        let repo: AgentStateRepository<State> =
            AgentStateRepository::new(temp_dir.utf8_path_buf(), "current-operation");

        let data = repo.load().await.unwrap();
        assert_eq!(data, None);
    }

    #[tokio::test]
    async fn agent_state_repository_exists_store() {
        let temp_dir = TempTedgeDir::new();
        temp_dir.dir(".agent").file("current-operation");

        let repo: AgentStateRepository<State> =
            AgentStateRepository::new(temp_dir.utf8_path_buf(), "current-operation");

        repo.store(&State {
            operation_id: "1234".to_string(),
            operation: "list".to_string(),
        })
        .await
        .unwrap();

        let data = tokio::fs::read_to_string(&format!(
            "{}/.agent/current-operation",
            &temp_dir.temp_dir.path().to_str().unwrap()
        ))
        .await
        .unwrap();

        assert_eq!(data, r#"{"operation_id":"1234","operation":"list"}"#)
    }
}
