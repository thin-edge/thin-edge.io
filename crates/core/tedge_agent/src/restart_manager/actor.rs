use crate::restart_manager::config::RestartManagerConfig;
use crate::restart_manager::error::RestartManagerError;
use crate::restart_manager::restart_operation_handler::restart_operation::create_tmp_restart_file;
use crate::restart_manager::restart_operation_handler::restart_operation::has_rebooted;
use crate::state_repository::state::AgentStateRepository;
use crate::state_repository::state::RestartOperationStatus;
use crate::state_repository::state::State;
use crate::state_repository::state::StateRepository;
use crate::state_repository::state::StateStatus;
use async_trait::async_trait;
use std::time::Duration;
use tedge_actors::Actor;
use tedge_actors::ChannelError;
use tedge_actors::MessageReceiver;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeRequest;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use tedge_api::OperationStatus;
use tedge_api::RestartOperationRequest;
use tedge_api::RestartOperationResponse;
use tedge_config::system_services::SystemConfig;
use tokio::process::Command;
use tokio::time::timeout;
use tracing::error;
use tracing::info;

const SYNC: &str = "sync";

#[cfg(not(test))]
const SUDO: &str = "sudo";
#[cfg(test)]
const SUDO: &str = "echo";

pub struct RestartManagerActor {
    config: RestartManagerConfig,
    state_repository: AgentStateRepository,
    message_box: SimpleMessageBox<RestartOperationRequest, RestartOperationResponse>,
}

#[async_trait]
impl Actor for RestartManagerActor {
    fn name(&self) -> &str {
        "RestartManagerActor"
    }

    async fn run(&mut self) -> Result<(), RuntimeError> {
        if let Some(response) = self.process_pending_restart_operation().await {
            self.message_box.send(response).await?;
        }

        while let Some(request) = self.message_box.recv().await {
            let executing_response = self.update_state_repository(&request).await;
            self.message_box.send(executing_response).await?;

            let maybe_error = self.handle_restart_operation().await;

            match timeout(Duration::from_secs(5), self.message_box.recv_signal()).await {
                Ok(Some(RuntimeRequest::Shutdown)) => {
                    // As expected, the restart triggered a shutdown.
                    return Ok(());
                }
                Ok(None) | Err(_) => {
                    // Something went wrong. The process should have been shutdown by the restart.
                    if let Err(err) = maybe_error {
                        error!("{}", err);
                    }
                    self.handle_error(&request).await?;
                }
            }
        }

        Ok(())
    }
}

impl RestartManagerActor {
    pub fn new(
        config: RestartManagerConfig,
        message_box: SimpleMessageBox<RestartOperationRequest, RestartOperationResponse>,
    ) -> Self {
        let state_repository = AgentStateRepository::new_with_file_name(
            config.config_dir.clone(),
            "restart-current-operation",
        );
        Self {
            config,
            state_repository,
            message_box,
        }
    }

    async fn process_pending_restart_operation(&mut self) -> Option<RestartOperationResponse> {
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
            self.clear_state_repository().await;

            match operation {
                StateStatus::Restart(RestartOperationStatus::Restarting) => {
                    let status = match has_rebooted(&self.config.tmp_dir) {
                        Ok(true) => {
                            info!("Device restart successful.");
                            OperationStatus::Successful
                        }
                        Ok(false) => {
                            info!("Device failed to restart.");
                            OperationStatus::Failed
                        }
                        Err(err) => {
                            error!("Fail to detect a restart: {err}");
                            OperationStatus::Failed
                        }
                    };

                    return Some(RestartOperationResponse { id, status });
                }
                StateStatus::Restart(RestartOperationStatus::Pending) => {
                    error!("The agent has been restarted but not the device");
                    let status = OperationStatus::Failed;
                    return Some(RestartOperationResponse { id, status });
                }
                StateStatus::Software(_) | StateStatus::UnknownOperation => {
                    error!("UnknownOperation in store.");
                }
            };
        }
        None
    }

    async fn update_state_repository(
        &mut self,
        request: &RestartOperationRequest,
    ) -> RestartOperationResponse {
        let response = RestartOperationResponse::new(request);
        let state = State {
            operation_id: Some(request.id.clone()),
            operation: Some(StateStatus::Restart(RestartOperationStatus::Restarting)),
        };

        if let Err(err) = self.state_repository.store(&state).await {
            error!(
                "Fail to update the restart state in {} due to: {}",
                self.state_repository.state_repo_path, err
            );
            return response.with_status(OperationStatus::Failed);
        }

        response
    }

    async fn handle_restart_operation(&mut self) -> Result<(), RestartManagerError> {
        create_tmp_restart_file(&self.config.tmp_dir)?;

        let commands = self.get_restart_operation_commands().await?;
        for mut command in commands {
            match command.status().await {
                Ok(status) => {
                    if !status.success() {
                        return Err(RestartManagerError::CommandFailed);
                    }
                }
                Err(e) => {
                    return Err(RestartManagerError::FromIo(e));
                }
            }
        }

        Ok(())
    }

    async fn handle_error(
        &mut self,
        request: &RestartOperationRequest,
    ) -> Result<(), ChannelError> {
        self.clear_state_repository().await;
        let status = OperationStatus::Failed;
        let response = RestartOperationResponse::new(request).with_status(status);
        self.message_box.send(response).await?;
        Ok(())
    }

    async fn clear_state_repository(&mut self) {
        if let Err(err) = self.state_repository.clear().await {
            error!(
                "Fail to clear the restart state in {} due to: {}",
                self.state_repository.state_repo_path, err
            );
        }
    }

    async fn get_restart_operation_commands(&self) -> Result<Vec<Command>, RestartManagerError> {
        let mut vec = vec![];
        // sync first
        let mut sync_command = Command::new(SUDO);
        sync_command.arg(SYNC);
        vec.push(sync_command);

        // reading `config_dir` to get the restart command or defaulting to `["init", "6"]'
        let system_config = SystemConfig::try_new(&self.config.config_dir)?;

        let mut command = Command::new(SUDO);
        command.args(system_config.system.reboot);
        vec.push(command);
        Ok(vec)
    }
}
