use crate::agent::TEDGE_AGENT;
use crate::software_manager::config::SoftwareManagerConfig;
use crate::software_manager::error::SoftwareManagerError;
use crate::software_manager::error::SoftwareManagerError::NoPlugins;
use crate::state_repository::error::StateError;
use crate::state_repository::state::AgentStateRepository;
use anyhow::anyhow;
use async_trait::async_trait;
use plugin_sm::plugin_manager::ExternalPlugins;
use plugin_sm::plugin_manager::Plugins;
use serde::Deserialize;
use serde::Serialize;
use std::process::Command;
use std::time::Duration;
use tedge_actors::fan_in_message_type;
use tedge_actors::Actor;
use tedge_actors::LoggingReceiver;
use tedge_actors::LoggingSender;
use tedge_actors::MessageReceiver;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeRequest;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use tedge_api::commands::CommandStatus;
use tedge_api::commands::SoftwareCommandMetadata;
use tedge_api::commands::SoftwareListCommand;
use tedge_api::commands::SoftwareUpdateCommand;
use tedge_api::mqtt_topics::OperationType;
use tedge_api::workflow::GenericCommandData;
use tedge_api::workflow::GenericCommandMetadata;
use tedge_api::workflow::GenericCommandState;
use tedge_api::CommandLog;
use tedge_api::Jsonify;
use tedge_api::SoftwareType;
use tedge_config::TEdgeConfigError;
use tracing::error;
use tracing::info;
use tracing::warn;

fan_in_message_type!(SoftwareCommand[SoftwareUpdateCommand, SoftwareListCommand, SoftwareCommandMetadata] : Debug, Eq, PartialEq, Deserialize, Serialize);

impl SoftwareCommand {
    pub fn into_generic_commands(self) -> Vec<GenericCommandData> {
        match self {
            SoftwareCommand::SoftwareUpdateCommand(cmd) => {
                vec![GenericCommandState::from(cmd).into()]
            }
            SoftwareCommand::SoftwareListCommand(cmd) => {
                vec![GenericCommandState::from(cmd).into()]
            }
            SoftwareCommand::SoftwareCommandMetadata(metadata) => {
                vec![
                    GenericCommandMetadata {
                        operation: OperationType::SoftwareList.to_string(),
                        payload: metadata.to_value(),
                    }
                    .into(),
                    GenericCommandMetadata {
                        operation: OperationType::SoftwareUpdate.to_string(),
                        payload: metadata.to_value(),
                    }
                    .into(),
                ]
            }
        }
    }
}

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
        let mut plugins = ExternalPlugins::open(
            &self.config.sm_plugins_dir,
            self.config.default_plugin_type.clone(),
            self.config.sudo.clone(),
            self.config.config_location.clone(),
        )
        .await
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

        self.output_sender
            .send(SoftwareCommand::SoftwareCommandMetadata(
                SoftwareCommandMetadata {
                    types: plugins.get_all_software_types(),
                },
            ))
            .await?;

        while let Some(request) = input_receiver.recv().await {
            tokio::select! {
                _ = self.handle_request(request, &mut plugins) => {
                    if let Err(SoftwareManagerError::NotRunningLatestVersion) = Self::detect_self_update() {
                        warn!("Tedge-agent is no more running the latest-version => a restart is required");
                        // Make sure the operation status is properly reported before the restart
                        tokio::time::sleep(Duration::from_secs(5)).await;
                        return Err(RuntimeError::ActorError(Box::new(SoftwareManagerError::NotRunningLatestVersion)));
                    }
                }

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
    ) -> Result<(), SoftwareManagerError> {
        match request {
            SoftwareCommand::SoftwareUpdateCommand(request) => {
                match self
                    .handle_software_update_operation(request, plugins)
                    .await
                {
                    Ok(()) => {}
                    Err(err) => error!("{:?}", err),
                }
            }
            SoftwareCommand::SoftwareListCommand(request) => {
                if let Err(err) = self.handle_software_list_operation(request, plugins).await {
                    error!("{:?}", err);
                }
            }
            SoftwareCommand::SoftwareCommandMetadata(_) => {} // Not used as input
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
            Ok(Some(SoftwareCommand::SoftwareCommandMetadata(_))) => (), // not used in state repository
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
    ) -> Result<(), SoftwareManagerError> {
        if request.status() != CommandStatus::Scheduled {
            // Only handle commands in the scheduled state
            return Ok(());
        }

        plugins.load().await?;
        plugins.update_default(&get_default_plugin(&self.config.config_location).await?)?;

        self.state_repository.store(&request.clone().into()).await?;

        // Send 'executing'
        let executing_response = request.clone().with_status(CommandStatus::Executing);
        self.output_sender.send(executing_response.into()).await?;

        let command_log = request.payload.log_path.clone().map(|path| {
            CommandLog::from_log_path(
                path,
                OperationType::SoftwareUpdate.to_string(),
                request.cmd_id.clone(),
            )
        });
        let response = plugins
            .process(request, command_log, self.config.tmp_dir.as_std_path())
            .await;
        self.output_sender.send(response.into()).await?;

        self.state_repository.clear().await?;
        Ok(())
    }

    fn detect_self_update() -> Result<(), SoftwareManagerError> {
        info!("Checking if tedge got self updated");
        let current_running_version = env!("CARGO_PKG_VERSION");
        info!("Current running version: {}", current_running_version);

        let executable_path = std::env::current_exe()
            .map_err(|e| anyhow!("Failed to retrieve running executable path due to {}", e))?;
        let agent_binary_path = executable_path.parent().unwrap().join(TEDGE_AGENT);

        let output = Command::new(agent_binary_path)
            .args(["--version"])
            .output()
            .map_err(|e| anyhow!("Failed to fetch version of installed binary due to {}", e))?;
        if !output.status.success() {
            return Err(anyhow!(
                "Fetching version from installed binary failed with {}",
                String::from_utf8_lossy(&output.stderr)
            )
            .into());
        }

        let version_output = String::from_utf8_lossy(&output.stdout);
        let version_output_split: Vec<&str> = version_output.split_whitespace().collect();
        if let ["tedge-agent", installed_binary_version] = version_output_split.as_slice() {
            info!("Installed binary version: {}", installed_binary_version);
            if current_running_version != *installed_binary_version {
                info!("Self update detected. Requesting shutdown...");
                return Err(SoftwareManagerError::NotRunningLatestVersion);
            }
        } else {
            return Err(anyhow!("Unexpected version output: {:?}", version_output).into());
        }

        Ok(())
    }

    async fn handle_software_list_operation(
        &mut self,
        request: SoftwareListCommand,
        plugins: &ExternalPlugins,
    ) -> Result<(), SoftwareManagerError> {
        if request.status() != CommandStatus::Scheduled {
            // Only handle commands in the scheduled state
            return Ok(());
        }

        self.state_repository.store(&request.clone().into()).await?;

        // Send 'executing'
        let executing_response = request.clone().with_status(CommandStatus::Executing);
        self.output_sender.send(executing_response.into()).await?;

        let command_log = request.payload.log_path.clone().map(|path| {
            CommandLog::from_log_path(
                path,
                OperationType::SoftwareList.to_string(),
                request.cmd_id.clone(),
            )
        });

        let response = plugins.list(request, command_log).await;
        self.output_sender.send(response.into()).await?;

        self.state_repository.clear().await?;
        Ok(())
    }
}

async fn get_default_plugin(
    config_location: &tedge_config::TEdgeConfigLocation,
) -> Result<Option<SoftwareType>, TEdgeConfigError> {
    let tedge_config = tedge_config::TEdgeConfig::try_new(config_location).await?;

    Ok(tedge_config
        .software
        .plugin
        .default
        .clone()
        .or_none()
        .cloned())
}
