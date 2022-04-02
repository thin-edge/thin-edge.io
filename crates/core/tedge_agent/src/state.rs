use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[async_trait]
pub trait StateRepository {
    type Error;
    async fn load(&self) -> Result<State, Self::Error>;
    async fn store(&self, state: &State) -> Result<(), Self::Error>;
    async fn clear(&self) -> Result<State, Self::Error>;
    async fn update(&self, status: &StateStatus) -> Result<(), Self::Error>;
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum StateStatus {
    Software(SoftwareOperationVariants),
    Restart(RestartOperationStatus),
    UnknownOperation,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum SoftwareOperationVariants {
    List,
    Update,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum RestartOperationStatus {
    Pending,
    Restarting,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub operation_id: Option<String>,
    pub operation: Option<StateStatus>,
}

#[cfg(test)]
mod tests {
    use crate::journal::Journal;
    use crate::state::{
        RestartOperationStatus, SoftwareOperationVariants, State, StateRepository, StateStatus,
    };

    use tempfile::tempdir;

    #[tokio::test]
    async fn agent_state_repository_not_exists_fail() {
        let temp_dir = tempdir().unwrap();
        assert!(Journal::open(temp_dir.into_path()).await.is_err());
    }

    #[tokio::test]
    async fn agent_state_repository_exists_loads_some() {
        let temp_dir = tempdir().unwrap();

        let _ = tokio::fs::create_dir(temp_dir.path().join(".agent/")).await;
        let destination_path = temp_dir.path().join(".agent/current-operation");

        let content = r#"(
            state: (
                operation_id: Some("1234"),
                operation: Some(Software(List)),
            ),
            pending: [],
        )"#;

        let _ = tokio::fs::write(destination_path.clone(), content.as_bytes()).await;

        let repo = Journal::open(destination_path)
            .await
            .expect("the previous journal");

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
        let temp_dir = tempdir().unwrap();

        let _ = tokio::fs::create_dir(temp_dir.path().join(".agent/")).await;
        let destination_path = temp_dir.path().join(".agent/current-operation");
        let content = r#"(
            state: (
                operation_id: Some("1234"),
                operation: Some(Restart(Restarting)),
            ),
            pending: [],
        )"#;
        let _ = tokio::fs::write(destination_path.clone(), content.as_bytes()).await;

        let repo = Journal::open(destination_path)
            .await
            .expect("the previous journal");

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
        let temp_dir = tempdir().unwrap();

        let _ = tokio::fs::create_dir(temp_dir.path().join(".agent/")).await;
        let destination_path = temp_dir.path().join(".agent/current-operation");

        let repo = Journal::open(destination_path)
            .await
            .expect("the previous journal");

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

        let _ = tokio::fs::create_dir(temp_dir.path().join(".agent/")).await;
        let destination_path = temp_dir.path().join(".agent/current-operation");

        let repo = Journal::open(destination_path.clone())
            .await
            .expect("the previous journal");

        repo.store(&State {
            operation_id: Some("1234".into()),
            operation: Some(StateStatus::Software(SoftwareOperationVariants::List)),
        })
        .await
        .unwrap();

        let data = tokio::fs::read_to_string(destination_path).await.unwrap();

        assert_eq!(
            data.split_ascii_whitespace().collect::<String>(),
            r#"(
            state: (
                operation_id: Some("1234"),
                operation: Some(Software(List)),
            ),
            pending: [],
        )"#
            .split_ascii_whitespace()
            .collect::<String>()
        );
    }
}
