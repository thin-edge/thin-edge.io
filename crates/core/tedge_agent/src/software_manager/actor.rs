use crate::software_manager::config::SoftwareManagerConfig;
use crate::software_manager::error::SoftwareManagerError;
use crate::software_manager::error::SoftwareManagerError::NoPlugins;
use crate::state_repository::error::StateError;
use crate::state_repository::state::AgentStateRepository;
use async_trait::async_trait;
use plugin_sm::operation_logs::LogKind;
use plugin_sm::operation_logs::OperationLogs;
use plugin_sm::plugin_manager::ExternalPlugins;
use plugin_sm::plugin_manager::Plugins;
use serde::Deserialize;
use serde::Serialize;
use std::path::PathBuf;
use tedge_actors::fan_in_message_type;
use tedge_actors::Actor;
use tedge_actors::LoggingReceiver;
use tedge_actors::LoggingSender;
use tedge_actors::MessageReceiver;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeRequest;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use tedge_api::messages::CommandStatus;
use tedge_api::messages::SoftwareListCommand;
use tedge_api::messages::SoftwareUpdateCommand;
use tedge_api::SoftwareType;
use tedge_config::TEdgeConfigError;
use tracing::error;
use tracing::info;
use tracing::warn;
use which::which;

#[cfg(not(test))]
const SUDO: &str = "sudo";
#[cfg(test)]
const SUDO: &str = "echo";

fan_in_message_type!(SoftwareCommand[SoftwareUpdateCommand, SoftwareListCommand] : Debug, Eq, PartialEq, Deserialize, Serialize);

/// Actor which performs software operations.
///
/// This actor takes as input [`SoftwareRequest`]s, and responds with
/// [`SoftwareResponse`]es. It mainly lists and updates software. It can only
/// process only a single [`SoftwareRequest`] at a time. On startup, it checks
/// if there are any leftover operations from a previous run, and if so, marks
/// them as failed.
///
/// Upon receiving a shutdown request, it will abort currently running
/// operation.
pub struct SoftwareManagerActor {
    config: SoftwareManagerConfig,
    state_repository: AgentStateRepository<SoftwareCommand>,

    // the Option is necessary to be able to concurrently handle a request,
    // which mutably borrows the sender, and listen on signals, which mutably
    // borrows the receiver. By using the Option we can take its contents
    // leaving a None in its place.
    //
    // If Actor::run signature was changed to consume self instead, we could
    // freely move out the receiver and get rid of the Option.
    //
    // https://github.com/thin-edge/thin-edge.io/pull/2049#discussion_r1243296392
    input_receiver: Option<LoggingReceiver<SoftwareCommand>>,
    output_sender: LoggingSender<SoftwareCommand>,
}

#[async_trait]
impl Actor for SoftwareManagerActor {
    fn name(&self) -> &str {
        "SoftwareManagerActor"
    }

    async fn run(mut self) -> Result<(), RuntimeError> {
        let operation_logs = OperationLogs::try_new(self.config.log_dir.clone().into())
            .map_err(SoftwareManagerError::FromOperationsLogs)?;

        let sudo: Option<PathBuf> = which(SUDO).ok();

        let mut plugins = ExternalPlugins::open(
            &self.config.sm_plugins_dir,
            self.config.default_plugin_type.clone(),
            sudo,
            self.config.config_location.clone(),
        )
        .map_err(|err| RuntimeError::ActorError(Box::new(err)))?;

        if plugins.empty() {
            warn!(
                "{}",
                NoPlugins {
                    plugins_path: self.config.sm_plugins_dir.clone(),
                }
            );
        }

        self.process_pending_sm_operation().await?;

        let mut input_receiver = self.input_receiver.take().ok_or(RuntimeError::ActorError(
            anyhow::anyhow!("actor can't be run more than once").into(),
        ))?;

        while let Some(request) = input_receiver.recv().await {
            tokio::select! {
                _ = self.handle_request(request, &mut plugins, &operation_logs) => {}

                Some(RuntimeRequest::Shutdown) = input_receiver.recv_signal() => {
                    info!("Received shutdown request from the runtime, exiting...");
                    // Here we could call `process_pending_sm_operation` to mark
                    // the current operation as failed, but OperationConverter
                    // also exited and we could hit filesystem-related race
                    // conditions due to concurrently executing
                    // `handle_request`, so we just exit for now
                    break;
                }
            }
        }

        Ok(())
    }
}

impl SoftwareManagerActor {
    pub fn new(
        config: SoftwareManagerConfig,
        message_box: SimpleMessageBox<SoftwareCommand, SoftwareCommand>,
    ) -> Self {
        let state_repository = AgentStateRepository::new(
            config.state_dir.clone(),
            config.config_dir.clone(),
            "software-current-operation",
        );
        let (output_sender, input_receiver) = message_box.into_split();

        Self {
            config,
            state_repository,
            input_receiver: Some(input_receiver),
            output_sender,
        }
    }

    async fn handle_request(
        &mut self,
        request: SoftwareCommand,
        plugins: &mut ExternalPlugins,
        operation_logs: &OperationLogs,
    ) -> Result<(), SoftwareManagerError> {
        match request {
            SoftwareCommand::SoftwareUpdateCommand(request) => {
                if let Err(err) = self
                    .handle_software_update_operation(request, plugins, operation_logs)
                    .await
                {
                    error!("{:?}", err);
                }
            }
            SoftwareCommand::SoftwareListCommand(request) => {
                if let Err(err) = self
                    .handle_software_list_operation(request, plugins, operation_logs)
                    .await
                {
                    error!("{:?}", err);
                }
            }
        }
        Ok(())
    }

    async fn process_pending_sm_operation(&mut self) -> Result<(), SoftwareManagerError> {
        match self.state_repository.load().await {
            Ok(Some(SoftwareCommand::SoftwareUpdateCommand(request))) => {
                let response = request.with_error(
                    "Software Update command cancelled due to unexpected agent restart".to_string(),
                );
                self.output_sender.send(response.into()).await?;
            }
            Ok(Some(SoftwareCommand::SoftwareListCommand(request))) => {
                let response = request.with_error(
                    "Software List request cancelled due to unexpected agent restart".to_string(),
                );
                self.output_sender.send(response.into()).await?;
            }
            Err(StateError::LoadingFromFileFailed { source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                // file missing means the operation has never been performed, so just do nothing
            }
            Err(err) => {
                // if read failed for some other reason, we should probably log it
                error!("{err}");
            }
            Ok(None) => (),
        };
        self.state_repository.clear().await?;
        Ok(())
    }

    async fn handle_software_update_operation(
        &mut self,
        request: SoftwareUpdateCommand,
        plugins: &mut ExternalPlugins,
        operation_logs: &OperationLogs,
    ) -> Result<(), SoftwareManagerError> {
        if request.status() != CommandStatus::Scheduled {
            // Only handle commands in the scheduled state
            return Ok(());
        }

        plugins.load()?;
        plugins.update_default(&get_default_plugin(&self.config.config_location)?)?;

        self.state_repository.store(&request.clone().into()).await?;

        // Send 'executing'
        let executing_response = request.clone().with_status(CommandStatus::Executing);
        self.output_sender.send(executing_response.into()).await?;

        let response = match operation_logs.new_log_file(LogKind::SoftwareUpdate).await {
            Ok(log_file) => {
                plugins
                    .process(request, log_file, self.config.tmp_dir.as_std_path())
                    .await
            }
            Err(err) => {
                error!("{}", err);
                request.with_error(format!("{}", err))
            }
        };
        self.output_sender.send(response.into()).await?;

        self.state_repository.clear().await?;
        Ok(())
    }

    async fn handle_software_list_operation(
        &mut self,
        request: SoftwareListCommand,
        plugins: &ExternalPlugins,
        operation_logs: &OperationLogs,
    ) -> Result<(), SoftwareManagerError> {
        if request.status() != CommandStatus::Scheduled {
            // Only handle commands in the scheduled state
            return Ok(());
        }

        self.state_repository.store(&request.clone().into()).await?;

        // Send 'executing'
        let executing_response = request.clone().with_status(CommandStatus::Executing);
        self.output_sender.send(executing_response.into()).await?;

        let response = match operation_logs.new_log_file(LogKind::SoftwareList).await {
            Ok(log_file) => plugins.list(request, log_file).await,
            Err(err) => {
                error!("{}", err);
                request.with_error(format!("{}", err))
            }
        };
        self.output_sender.send(response.into()).await?;

        self.state_repository.clear().await?;
        Ok(())
    }
}

fn get_default_plugin(
    config_location: &tedge_config::TEdgeConfigLocation,
) -> Result<Option<SoftwareType>, TEdgeConfigError> {
    let config_repository = tedge_config::TEdgeConfigRepository::new(config_location.clone());
    let tedge_config = config_repository.load()?;

    Ok(tedge_config
        .software
        .plugin
        .default
        .clone()
        .or_none()
        .cloned())
}
