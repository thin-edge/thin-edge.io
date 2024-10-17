use crate::state_repository::error::StateError;
use camino::Utf8PathBuf;
use log::info;
use log::warn;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fs::File;
use std::io::Write;
use std::marker::PhantomData;
use tedge_utils::fs::atomically_write_file_async;
use tokio::fs;

/// Store the current state of an operation
#[derive(Debug)]
pub struct AgentStateRepository<T> {
    pub state_repo_path: Utf8PathBuf,
    phantom: PhantomData<T>,
}

/// The directory used by the agent to persist its state when tedge config agent.state.path is not set
pub fn agent_default_state_dir(tedge_root: Utf8PathBuf) -> Utf8PathBuf {
    tedge_root.join(".agent")
}

/// Return the given `state_dir` once checked that it can be used to persist the agent state.
///
/// If for some reason the configured state directory cannot be used,
/// then fallback to the default directory under tedge root (`/etc/tedge/.agent`).
pub fn agent_state_dir(state_dir: Utf8PathBuf, tedge_root: Utf8PathBuf) -> Utf8PathBuf {
    // Check that the given directory is actually writable, by creating an empty test file
    let test_file = state_dir.join(state_dir.join(".--test--"));
    match File::create(test_file.clone()).and_then(|mut file| file.write_all(b"")) {
        Ok(_) => {
            let _ = std::fs::remove_file(test_file);
            state_dir
        }
        Err(err) => {
            warn!("Cannot use {state_dir:?} to store tedge-agent state: {err}");
            agent_default_state_dir(tedge_root)
        }
    }
}

impl<T: DeserializeOwned + Serialize> AgentStateRepository<T> {
    /// Create a new agent state file in the given state directory
    /// or in the tedge root directory if the given directory is not suitable
    /// (e.g. the directory doesn't exist or is not writable).
    pub fn new(state_dir: Utf8PathBuf, tedge_root: Utf8PathBuf, file_name: &str) -> Self {
        let state_dir = agent_state_dir(state_dir, tedge_root);
        Self::with_state_dir(state_dir, file_name)
    }

    /// Create a new agent state file in the given state directory.
    pub fn with_state_dir(state_dir: Utf8PathBuf, file_name: &str) -> Self {
        let state_repo_path = state_dir.join(file_name);
        info!("Use {state_repo_path:?} to store tedge-agent {file_name} state");
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

    fn new_test_state_repository(temp_dir: &TempTedgeDir) -> AgentStateRepository<State> {
        AgentStateRepository::new(
            "/some/unknown/dir".into(),
            temp_dir.utf8_path_buf(),
            "current-operation",
        )
    }

    #[tokio::test]
    async fn use_given_directory_if_it_exist() {
        let temp_dir = TempTedgeDir::new();
        temp_dir
            .file("current-operation")
            .with_raw_content(r#"{"operation_id":"1234","operation":"list"}"#);
        let repo: AgentStateRepository<State> = AgentStateRepository::new(
            temp_dir.utf8_path_buf(),
            "/some/unknown/dir".into(),
            "current-operation",
        );

        assert_eq!(
            repo.load().await.unwrap(),
            Some(State {
                operation_id: "1234".to_string(),
                operation: "list".to_string()
            })
        );
    }

    #[tokio::test]
    async fn fall_back_to_default_agent_state_repository_if_given_directory_does_not_exist() {
        let temp_dir = TempTedgeDir::new();
        temp_dir.dir(".agent").file("current-operation");
        let repo: AgentStateRepository<State> = AgentStateRepository::new(
            "/some/unknown/dir".into(),
            temp_dir.utf8_path_buf(),
            "current-operation",
        );

        assert_eq!(repo.load().await.unwrap(), None);
    }

    #[tokio::test]
    async fn fail_when_none_of_the_given_directory_exist() {
        let temp_dir = TempTedgeDir::new();
        let repo: AgentStateRepository<State> = AgentStateRepository::new(
            "/some/unknown/dir".into(),
            temp_dir.utf8_path_buf(),
            "current-operation",
        );

        assert!(repo.load().await.is_err());
    }

    #[tokio::test]
    async fn agent_state_repository_exists_loads_some() {
        let temp_dir = TempTedgeDir::new();
        let content = r#"{"operation_id":"1234","operation":"list"}"#;
        temp_dir
            .dir(".agent")
            .file("current-operation")
            .with_raw_content(content);
        let repo = new_test_state_repository(&temp_dir);

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
        let repo = new_test_state_repository(&temp_dir);

        let data = repo.load().await.unwrap();
        assert_eq!(data, None);
    }

    #[tokio::test]
    async fn agent_state_repository_exists_store() {
        let temp_dir = TempTedgeDir::new();
        temp_dir.dir(".agent").file("current-operation");
        let repo = new_test_state_repository(&temp_dir);

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
