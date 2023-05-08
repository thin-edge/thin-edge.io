use crate::software_list_manager::config::SoftwareListManagerConfig;
use crate::software_list_manager::error::SoftwareListManagerError;
use crate::software_list_manager::error::SoftwareListManagerError::NoPlugins;
use crate::state_repository::state::AgentStateRepository;
use crate::state_repository::state::SoftwareOperationVariants;
use crate::state_repository::state::State;
use crate::state_repository::state::StateRepository;
use crate::state_repository::state::StateStatus;
use async_trait::async_trait;
use plugin_sm::operation_logs::LogKind;
use plugin_sm::operation_logs::OperationLogs;
use plugin_sm::plugin_manager::ExternalPlugins;
use std::sync::Arc;
use tedge_actors::Actor;
use tedge_actors::MessageReceiver;
use tedge_actors::RuntimeError;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use tedge_api::OperationStatus;
use tedge_api::SoftwareListRequest;
use tedge_api::SoftwareListResponse;
use tedge_api::SoftwareRequestResponse;
use tokio::sync::Mutex;
use tracing::error;
use tracing::log::warn;

#[cfg(not(test))]
const SUDO: &str = "sudo";
#[cfg(test)]
const SUDO: &str = "echo";

pub struct SoftwareListManagerActor {
    config: SoftwareListManagerConfig,
    state_repository: AgentStateRepository,
    operation_logs: OperationLogs,
    message_box: SimpleMessageBox<SoftwareListRequest, SoftwareListResponse>,
}

#[async_trait]
impl Actor for SoftwareListManagerActor {
    fn name(&self) -> &str {
        "SoftwareListManagerActor"
    }

    async fn run(&mut self) -> Result<(), RuntimeError> {
        let plugins = Arc::new(Mutex::new(
            ExternalPlugins::open(
                &self.config.sm_plugins_dir,
                self.config.default_plugin_type.clone(),
                Some(SUDO.into()),
            )
            .unwrap(), // TODO: Fix this unwrap
        ));

        if plugins.lock().await.empty() {
            warn!(
                "{}",
                NoPlugins {
                    plugins_path: self.config.sm_plugins_dir.clone(),
                }
            );
        }

        self.process_pending_sm_list_operation().await?;

        while let Some(request) = self.message_box.recv().await {
            if let Err(err) = self
                .handle_software_list_operation(&request, plugins.clone())
                .await
            {
                error!("{:?}", err);
            }
        }
        Ok(())
    }
}

impl SoftwareListManagerActor {
    pub fn new(
        config: SoftwareListManagerConfig,
        message_box: SimpleMessageBox<SoftwareListRequest, SoftwareListResponse>,
    ) -> Self {
        let state_repository = AgentStateRepository::new_with_file_name(
            config.config_dir.clone(),
            "software-list-current-operation",
        );
        let operation_logs = OperationLogs::try_new(config.log_dir.clone().into()).unwrap(); // TODO: Fix this unwrap

        Self {
            config,
            state_repository,
            operation_logs,
            message_box,
        }
    }

    async fn process_pending_sm_list_operation(&mut self) -> Result<(), SoftwareListManagerError> {
        let state: Result<State, _> = self.state_repository.load().await;

        if let State {
            operation_id: Some(id),
            operation: Some(operation),
        } = match state {
            Ok(state) => state,
            Err(_) => State {
                operation_id: None,
                operation: None,
            },
        } {
            match operation {
                StateStatus::Software(SoftwareOperationVariants::List) => {
                    let response = SoftwareRequestResponse::new(&id, OperationStatus::Failed);
                    self.message_box
                        .send(SoftwareListResponse { response })
                        .await?;
                }
                StateStatus::Restart(_)
                | StateStatus::Software(SoftwareOperationVariants::Update) => {
                    error!("No SoftwareListOperation in store.");
                }
                StateStatus::UnknownOperation => {
                    error!("UnknownOperation in store.");
                }
            };
        }
        Ok(())
    }

    async fn handle_software_list_operation(
        &mut self,
        request: &SoftwareListRequest,
        plugins: Arc<Mutex<ExternalPlugins>>,
    ) -> Result<(), SoftwareListManagerError> {
        self.state_repository
            .store(&State {
                operation_id: Some(request.id.clone()),
                operation: Some(StateStatus::Software(SoftwareOperationVariants::List)),
            })
            .await?;

        // Send 'executing'
        let executing_response = SoftwareListResponse::new(request);
        self.message_box.send(executing_response).await?;

        let response = match self
            .operation_logs
            .new_log_file(LogKind::SoftwareList)
            .await
        {
            Ok(log_file) => plugins.lock().await.list(request, log_file).await,
            Err(err) => {
                error!("{}", err);
                let mut failed_response = SoftwareListResponse::new(request);
                failed_response.set_error(&format!("{}", err));
                failed_response
            }
        };
        self.message_box.send(response).await?;

        let _state: State = self.state_repository.clear().await?;

        Ok(())
    }
}
