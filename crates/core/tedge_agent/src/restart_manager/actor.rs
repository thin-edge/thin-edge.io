use crate::restart_manager::error::RestartManagerError;
use crate::restart_manager::restart_operation_handler::restart_operation::create_tmp_restart_file;
use crate::restart_manager::restart_operation_handler::restart_operation::has_rebooted;
use crate::state_repository::state::AgentStateRepository;
use crate::state_repository::state::RestartOperationStatus;
use crate::state_repository::state::State;
use crate::state_repository::state::StateRepository;
use crate::state_repository::state::StateStatus;
use async_trait::async_trait;
use camino::Utf8PathBuf;
use tedge_actors::Actor;
use tedge_actors::DynSender;
use tedge_actors::LoggingReceiver;
use tedge_actors::MessageReceiver;
use tedge_actors::RuntimeError;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_api::OperationStatus;
use tedge_api::RestartOperationRequest;
use tedge_api::RestartOperationResponse;
use tedge_config::system_services::SystemConfig;
use tokio::process::Command;
use tracing::error;
use tracing::info;

const SYNC: &str = "sync";

#[cfg(not(test))]
const SUDO: &str = "sudo";
#[cfg(test)]
const SUDO: &str = "echo";

#[derive(Debug)]
pub struct RestartManagerConfig {
    pub tmp_dir: Utf8PathBuf,
    pub tedge_root_path: Utf8PathBuf,
    pub system_config_path: Utf8PathBuf,
}

impl RestartManagerConfig {
    pub fn new(
        tmp_dir: Utf8PathBuf,
        tedge_root_path: Utf8PathBuf,
        system_config_path: Utf8PathBuf,
    ) -> Self {
        Self {
            tmp_dir,
            tedge_root_path,
            system_config_path,
        }
    }
}

pub struct RestartManagerActor {
    config: RestartManagerConfig,
    state_repository: AgentStateRepository,
    message_box: SimpleMessageBox<RestartOperationRequest, RestartOperationResponse>,
    // input_receiver: LoggingReceiver<RestartOperationRequest>,
    // converter_sender: DynSender<RestartOperationResponse>,
}

#[async_trait]
impl Actor for RestartManagerActor {
    fn name(&self) -> &str {
        "RestartManagerActor"
    }

    async fn run(&mut self) -> Result<(), RuntimeError> {
        // kind of 'init'
        self.process_pending_restart_operation().await?;

        while let Some(request) = self.message_box.recv().await {
            if let Err(err) = self.handle_restart_operation(&request).await {
                error!("{:?}", err);
                self.handle_error(&request).await?;
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
        let state_repository = AgentStateRepository::new(config.tedge_root_path.clone());
        Self {
            config,
            state_repository,
            message_box,
        }
    }

    async fn process_pending_restart_operation(&mut self) -> Result<(), RestartManagerError> {
        let state: Result<State, _> = self.state_repository.load().await;

        let mut status = OperationStatus::Failed;

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
                StateStatus::Restart(RestartOperationStatus::Restarting) => {
                    let _state = self.state_repository.clear().await?;

                    if has_rebooted(&self.config.tmp_dir)? {
                        info!("Device restart successful.");
                        status = OperationStatus::Successful;
                    }
                }
                StateStatus::Restart(RestartOperationStatus::Pending) => {}
                StateStatus::UnknownOperation => {
                    error!("UnknownOperation in store.");
                }
                _ => {
                    unimplemented!()
                }
            };
            self.message_box
                .send(RestartOperationResponse { id, status })
                .await
                .unwrap();
        }
        Ok(())
    }

    async fn handle_restart_operation(
        &mut self,
        request: &RestartOperationRequest,
    ) -> Result<(), RestartManagerError> {
        self.state_repository
            .store(&State {
                operation_id: Some(request.id.clone()),
                operation: Some(StateStatus::Restart(RestartOperationStatus::Restarting)),
            })
            .await?;

        // Send 'executing'
        let executing_response = RestartOperationResponse::new(&RestartOperationRequest::default());
        self.message_box.send(executing_response).await.unwrap();

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
    ) -> Result<(), RestartManagerError> {
        self.state_repository.clear().await?;
        let status = OperationStatus::Failed;
        let response = RestartOperationResponse::new(&request).with_status(status);
        self.message_box.send(response).await?;
        Ok(())
    }

    async fn get_restart_operation_commands(&self) -> Result<Vec<Command>, RestartManagerError> {
        let mut vec = vec![];
        // sync first
        let mut sync_command = Command::new(SUDO);
        sync_command.arg(SYNC);
        vec.push(sync_command);

        // reading `system_config_path` to get the restart command or defaulting to `["init", "6"]'
        let system_config = SystemConfig::try_new(&self.config.system_config_path)?;

        let mut command = Command::new(SUDO);
        command.args(system_config.system.reboot);
        vec.push(command);
        Ok(vec)
    }
}
