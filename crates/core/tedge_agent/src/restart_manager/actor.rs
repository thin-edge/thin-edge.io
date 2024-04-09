use crate::restart_manager::config::RestartManagerConfig;
use crate::restart_manager::error::RestartManagerError;
use crate::restart_manager::restart_operation_handler::restart_operation::create_tmp_restart_file;
use crate::restart_manager::restart_operation_handler::restart_operation::has_rebooted;
use crate::state_repository::error::StateError;
use crate::state_repository::state::AgentStateRepository;
use async_trait::async_trait;
use std::time::Duration;
use tedge_actors::Actor;
use tedge_actors::ChannelError;
use tedge_actors::MessageReceiver;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeRequest;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use tedge_api::commands::CommandStatus;
use tedge_api::RestartCommand;
use tedge_config::system_services::SystemConfig;
use tedge_config::system_services::SystemSpecificCommands;
use tokio::process::Command;
use tokio::time::timeout;
use tracing::error;
use tracing::info;
use tracing::warn;

const SYNC: &str = "sync";

pub struct RestartManagerActor {
    config: RestartManagerConfig,
    state_repository: AgentStateRepository<RestartCommand>,
    message_box: SimpleMessageBox<RestartCommand, RestartCommand>,
}

#[async_trait]
impl Actor for RestartManagerActor {
    fn name(&self) -> &str {
        "RestartManagerActor"
    }

    async fn run(mut self) -> Result<(), RuntimeError> {
        if let Some(response) = self.process_pending_restart_operation().await {
            self.message_box.send(response).await?;
        }
        self.clear_state_repository().await;

        while let Some(request) = self.message_box.recv().await {
            if request.status() != CommandStatus::Scheduled {
                // Only handle commands in the scheduled state
                continue;
            }
            let executing_response = self.update_state_repository(request.clone()).await;
            let ready = executing_response.status() == CommandStatus::Executing;
            self.message_box.send(executing_response).await?;
            if !ready {
                info!("Cannot restart");
                continue;
            }
            info!("Triggering a restart");

            let restart_timeout = self.get_restart_timeout();
            match timeout(restart_timeout, self.handle_restart_operation()).await {
                Ok(Err(err)) => {
                    let error = format!("Fail to trigger a restart: {err}");
                    error!(error);
                    self.handle_error(request, error).await?;
                }
                Err(_) => {
                    let error = format!(
                        "Restart command still running after {} seconds",
                        restart_timeout.as_secs()
                    );
                    error!(error);
                    self.handle_error(request, error).await?;
                }
                Ok(Ok(not_interrupted)) => {
                    if not_interrupted {
                        info!("The restart command has been successfully executed");
                    } else {
                        info!("The restart command has been interrupted by a signal");
                    }
                    match timeout(restart_timeout, self.message_box.recv_signal()).await {
                        Ok(Some(RuntimeRequest::Shutdown)) => {
                            info!("As requested, a shutdown has been triggered");
                            return Ok(());
                        }
                        Ok(None) | Err(_ /* timeout */) => {
                            // Something went wrong. The process should have been shutdown by the restart.
                            let error = "No shutdown has been triggered".to_string();
                            error!(error);
                            self.handle_error(request, error).await?;
                        }
                    }
                }
            };
        }

        Ok(())
    }
}

impl RestartManagerActor {
    pub fn new(
        config: RestartManagerConfig,
        message_box: SimpleMessageBox<RestartCommand, RestartCommand>,
    ) -> Self {
        let state_repository = AgentStateRepository::new(
            config.state_dir.clone(),
            config.config_dir.clone(),
            "restart-current-operation",
        );
        Self {
            config,
            state_repository,
            message_box,
        }
    }

    async fn process_pending_restart_operation(&mut self) -> Option<RestartCommand> {
        match self.state_repository.load().await {
            Ok(Some(command)) if command.status() == CommandStatus::Executing => {
                let command = match has_rebooted(&self.config.tmp_dir) {
                    Ok(true) => {
                        info!("Device restart successful");
                        command.with_status(CommandStatus::Successful)
                    }
                    Ok(false) => {
                        let error = "Device failed to restart";
                        error!(error);
                        command.with_error(error.to_string())
                    }
                    Err(err) => {
                        let error = format!("Fail to detect a restart: {err}");
                        error!(error);
                        command.with_error(error)
                    }
                };

                Some(command)
            }
            Ok(Some(command)) => {
                let error = "The agent has been restarted but not the device";
                error!(error);
                Some(command.with_error(error.to_string()))
            }
            Err(StateError::LoadingFromFileFailed { source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                // file missing means the operation has never been performed, so just do nothing
                None
            }
            Err(err) => {
                // if read failed for some other reason, we should probably log it
                error!("{err}");
                None
            }
            Ok(None) => None,
        }
    }

    async fn update_state_repository(&mut self, command: RestartCommand) -> RestartCommand {
        let command = command.with_status(CommandStatus::Executing);
        if let Err(err) = self.state_repository.store(&command).await {
            let reason = format!(
                "Fail to update the restart state in {} due to: {}",
                self.state_repository.state_repo_path, err
            );
            error!(reason);
            return command.with_error(reason);
        }

        if let Err(err) = create_tmp_restart_file(&self.config.tmp_dir).await {
            let reason = format!(
                "Fail to create a witness file in {} due to: {}",
                self.config.tmp_dir, err
            );
            error!(reason);
            return command.with_error(reason);
        }

        command
    }

    /// Run the restart command
    ///
    /// Returns:
    /// - `Ok(true)` if all the commands run successfully.
    /// - `Ok(false)` if one of the commands has been interrupted by a signal.
    /// - `Err(_)` if one the commands cannot be launched or failed.
    async fn handle_restart_operation(&mut self) -> Result<bool, RestartManagerError> {
        let commands = self.get_restart_operation_commands()?;
        let mut not_interrupted = true;
        for mut command in commands {
            let cmd = command.as_std().get_program().to_string_lossy();
            let args = command.as_std().get_args();
            info!("Restarting: {cmd} {args:?}");

            match command.status().await {
                Ok(status) => {
                    if status.code().is_none() {
                        // This might the result of the reboot - hence not considered as an error
                        not_interrupted = false;
                    } else if !status.success() {
                        return Err(RestartManagerError::CommandFailed {
                            command: format!("{command:?}"),
                        });
                    }
                }
                Err(e) => {
                    return Err(RestartManagerError::FromIo(e));
                }
            }
        }

        Ok(not_interrupted)
    }

    async fn handle_error(
        &mut self,
        command: RestartCommand,
        reason: String,
    ) -> Result<(), ChannelError> {
        self.clear_state_repository().await;
        self.message_box.send(command.with_error(reason)).await?;
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

    fn get_restart_operation_commands(&self) -> Result<Vec<Command>, RestartManagerError> {
        let mut restart_commands = vec![];

        // reading `config_dir` to get the restart command or defaulting to `["init", "6"]'
        let system_config = SystemConfig::try_new(&self.config.config_dir)?;

        let sync_command = self.config.sudo.command(SYNC).into();
        restart_commands.push(sync_command);

        let Some((reboot_command, reboot_args)) = system_config.system.reboot.split_first() else {
            warn!("`system.reboot` is empty");
            return Ok(restart_commands);
        };

        let mut reboot_command: Command = self.config.sudo.command(reboot_command).into();
        reboot_command.args(reboot_args);
        restart_commands.push(reboot_command);

        Ok(restart_commands)
    }

    fn get_restart_timeout(&self) -> Duration {
        SystemConfig::try_new(&self.config.config_dir)
            .map(|config| config.system.reboot_timeout())
            .unwrap_or_else(|_| SystemSpecificCommands::default().reboot_timeout())
    }
}
