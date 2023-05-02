use crate::error::StateError;
use crate::state_repository::state::AgentStateRepository;
use crate::state_repository::state::State;
use crate::state_repository::state::StateRepository;
use crate::state_repository::state::StateStatus;
use async_trait::async_trait;
use camino::Utf8PathBuf;
use tedge_actors::Sequential;
use tedge_actors::Server;
use tedge_actors::ServerActorBuilder;
use tedge_actors::ServerConfig;
use tedge_actors::SimpleMessageBox;

pub type StateResult = Result<State, StateError>;

#[derive(Debug, Eq, PartialEq)]
pub enum StateRepositoryAction {
    Load,
    Store(State),
    Clear,
    Update(StateStatus),
}

#[derive(Debug, Default)]
pub struct StateRepositoryActor {
    config: ServerConfig,
    tedge_dir: Utf8PathBuf,
}

impl StateRepositoryActor {
    pub fn new(tedge_dir: Utf8PathBuf) -> Self {
        Self {
            config: Default::default(),
            tedge_dir,
        }
    }

    pub fn builder(self) -> ServerActorBuilder<StateRepositoryActor, Sequential> {
        ServerActorBuilder::new(self, &ServerConfig::new(), Sequential)
    }

    pub fn with_capacity(self, capacity: usize) -> Self {
        Self {
            config: self.config.with_capacity(capacity),
            tedge_dir: self.tedge_dir,
        }
    }
}

#[async_trait]
impl Server for StateRepositoryActor {
    type Request = StateRepositoryAction;
    type Response = StateResult;

    fn name(&self) -> &str {
        "StateRepository"
    }

    async fn handle(&mut self, request: Self::Request) -> Self::Response {
        let state_repository = AgentStateRepository::new(self.tedge_dir.clone());

        match request {
            StateRepositoryAction::Load => state_repository.load().await,
            StateRepositoryAction::Store(state) => state_repository.store(&state).await,
            StateRepositoryAction::Clear => state_repository.clear().await,
            StateRepositoryAction::Update(state_status) => {
                state_repository.update(&state_status).await
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state_repository::state::RestartOperationStatus;
    use crate::state_repository::state::SoftwareOperationVariants;
    use std::time::Duration;
    use tedge_actors::test_helpers::MessageReceiverExt;
    use tedge_actors::test_helpers::TimedMessageBox;
    use tedge_actors::Actor;
    use tedge_actors::Builder;
    use tedge_actors::ClientMessageBox;
    use tedge_actors::ConvertingActor;
    use tedge_actors::DynError;
    use tedge_actors::MessageReceiver;
    use tedge_actors::NoConfig;
    use tedge_actors::Sender;
    use tedge_actors::ServiceConsumer;
    use tedge_actors::SimpleMessageBox;
    use tedge_actors::SimpleMessageBoxBuilder;
    use tedge_test_utils::fs::TempTedgeDir;
    use tokio::time::timeout;

    const TEST_TIMEOUT: Duration = Duration::from_secs(5);

    #[tokio::test]
    async fn test_load() {
        let mut temp_dir = TempTedgeDir::new();
        let content = "operation_id = \'1234\'\noperation = \"list\"";
        temp_dir
            .dir(".agent")
            .file("current-operation")
            .with_raw_content(content);

        let mut client = spawn_state_repository_actor(&mut temp_dir).await;

        let state = timeout(
            TEST_TIMEOUT,
            client.await_response(StateRepositoryAction::Load),
        )
        .await
        .expect("timeout")
        .expect("channel error")
        .expect("state error");

        assert_eq!(
            state,
            State {
                operation_id: Some("1234".into()),
                operation: Some(StateStatus::Software(SoftwareOperationVariants::List)),
            }
        );
    }

    #[tokio::test]
    async fn test_store() {
        let mut temp_dir = TempTedgeDir::new();
        temp_dir.dir(".agent").file("current-operation");

        let mut client = spawn_state_repository_actor(&mut temp_dir).await;

        let new_state = State {
            operation_id: Some("1234".into()),
            operation: Some(StateStatus::Software(SoftwareOperationVariants::List)),
        };

        let state = timeout(
            TEST_TIMEOUT,
            client.await_response(StateRepositoryAction::Store(new_state.clone())),
        )
        .await
        .expect("timeout")
        .expect("channel error")
        .expect("state error");
        assert_eq!(state, new_state);

        let path = temp_dir.path().join(".agent").join("current-operation");
        let data = tokio::fs::read_to_string(path).await.unwrap();
        assert_eq!(data, "operation_id = \'1234\'\noperation = \'list\'\n");
    }

    #[tokio::test]
    async fn test_clear() {
        let mut temp_dir = TempTedgeDir::new();
        let content = "operation_id = \'1234\'\noperation = \"list\"";
        temp_dir
            .dir(".agent")
            .file("current-operation")
            .with_raw_content(content);

        let mut client = spawn_state_repository_actor(&mut temp_dir).await;

        let state = timeout(
            TEST_TIMEOUT,
            client.await_response(StateRepositoryAction::Clear),
        )
        .await
        .expect("timeout")
        .expect("channel error")
        .expect("state error");
        assert_eq!(
            state,
            State {
                operation_id: None,
                operation: None
            }
        );

        let path = temp_dir.path().join(".agent").join("current-operation");
        let data = tokio::fs::read_to_string(path).await.unwrap();
        assert_eq!(data, "");
    }

    #[tokio::test]
    async fn test_update() {
        let mut temp_dir = TempTedgeDir::new();
        let content = "operation_id = \'1234\'\noperation = \"Restarting\"";
        temp_dir
            .dir(".agent")
            .file("current-operation")
            .with_raw_content(content);

        let mut client = spawn_state_repository_actor(&mut temp_dir).await;

        let new_state_status = StateStatus::Restart(RestartOperationStatus::Pending);

        let state = timeout(
            TEST_TIMEOUT,
            client.await_response(StateRepositoryAction::Update(new_state_status)),
        )
        .await
        .expect("timeout")
        .expect("channel error")
        .expect("state error");

        dbg!(&state);
        assert_eq!(
            state,
            State {
                operation_id: Some("1234".to_string(),),
                operation: Some(StateStatus::Restart(RestartOperationStatus::Pending)),
            }
        );

        let path = temp_dir.path().join(".agent").join("current-operation");
        let data = tokio::fs::read_to_string(path).await.unwrap();
        assert_eq!(data, "operation_id = '1234'\noperation = 'Pending'\n");
    }

    async fn spawn_state_repository_actor(
        tmp_dir: &mut TempTedgeDir,
    ) -> ClientMessageBox<StateRepositoryAction, StateResult> {
        let mut state_repository_actor_builder =
            StateRepositoryActor::new(tmp_dir.utf8_path_buf()).builder();
        let client =
            ClientMessageBox::new("StateRepositoryClient", &mut state_repository_actor_builder);

        tokio::spawn(state_repository_actor_builder.run());

        client
    }
}
