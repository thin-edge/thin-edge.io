use crate::software_manager::config::SoftwareManagerConfig;
use crate::software_manager::error::SoftwareManagerError;
use crate::software_manager::error::SoftwareManagerError::NoPlugins;
use crate::state_repository::state::AgentStateRepository;
use crate::state_repository::state::SoftwareOperationVariants;
use crate::state_repository::state::State;
use crate::state_repository::state::StateRepository;
use crate::state_repository::state::StateStatus;
use async_trait::async_trait;
use plugin_sm::operation_logs::LogKind;
use plugin_sm::operation_logs::OperationLogs;
use plugin_sm::plugin_manager::ExternalPlugins;
use plugin_sm::plugin_manager::Plugins;
use std::sync::Arc;
use tedge_actors::fan_in_message_type;
use tedge_actors::Actor;
use tedge_actors::MessageReceiver;
use tedge_actors::RuntimeError;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use tedge_api::OperationStatus;
use tedge_api::SoftwareListRequest;
use tedge_api::SoftwareListResponse;
use tedge_api::SoftwareRequestResponse;
use tedge_api::SoftwareType;
use tedge_api::SoftwareUpdateRequest;
use tedge_api::SoftwareUpdateResponse;
use tedge_config::ConfigRepository;
use tedge_config::ConfigSettingAccessorStringExt;
use tedge_config::SoftwarePluginDefaultSetting;
use tedge_config::TEdgeConfigLocation;
use tokio::sync::Mutex;
use tracing::error;
use tracing::log::warn;

#[cfg(not(test))]
const SUDO: &str = "sudo";
#[cfg(test)]
const SUDO: &str = "echo";

fan_in_message_type!(SoftwareRequest[SoftwareUpdateRequest, SoftwareListRequest] : Debug, Eq, PartialEq);
fan_in_message_type!(SoftwareResponse[SoftwareUpdateResponse, SoftwareListResponse] : Debug, Eq, PartialEq);

pub struct SoftwareManagerActor {
    config: SoftwareManagerConfig,
    state_repository: AgentStateRepository,
    operation_logs: OperationLogs,
    message_box: SimpleMessageBox<SoftwareRequest, SoftwareResponse>,
}

#[async_trait]
impl Actor for SoftwareManagerActor {
    fn name(&self) -> &str {
        "SoftwareManagerActor"
    }

    async fn run(&mut self) -> Result<(), RuntimeError> {
        let plugins = Arc::new(Mutex::new(
            ExternalPlugins::open(
                &self.config.sm_plugins_dir,
                self.config.default_plugin_type.clone(),
                Some(SUDO.into()),
            )
            .map_err(|err| RuntimeError::ActorError(Box::new(err)))?,
        ));

        if plugins.lock().await.empty() {
            warn!(
                "{}",
                NoPlugins {
                    plugins_path: self.config.sm_plugins_dir.clone(),
                }
            );
        }

        self.process_pending_sm_operation().await?;

        while let Some(request) = self.message_box.recv().await {
            match request {
                SoftwareRequest::SoftwareUpdateRequest(request) => {
                    if let Err(err) = self
                        .handle_software_update_operation(&request, plugins.clone())
                        .await
                    {
                        error!("{:?}", err);
                    }
                }
                SoftwareRequest::SoftwareListRequest(request) => {
                    if let Err(err) = self
                        .handle_software_list_operation(&request, plugins.clone())
                        .await
                    {
                        error!("{:?}", err);
                    }
                }
            }
        }
        Ok(())
    }
}

impl SoftwareManagerActor {
    pub fn new(
        config: SoftwareManagerConfig,
        message_box: SimpleMessageBox<SoftwareRequest, SoftwareResponse>,
    ) -> Self {
        let state_repository = AgentStateRepository::new_with_file_name(
            config.config_dir.clone(),
            "software-current-operation",
        );
        let operation_logs = OperationLogs::try_new(config.log_dir.clone().into()).unwrap(); // TODO: Fix this unwrap

        Self {
            config,
            state_repository,
            operation_logs,
            message_box,
        }
    }

    async fn process_pending_sm_operation(&mut self) -> Result<(), SoftwareManagerError> {
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
                StateStatus::Software(SoftwareOperationVariants::Update) => {
                    let response = SoftwareRequestResponse::new(&id, OperationStatus::Failed);
                    self.message_box
                        .send(SoftwareUpdateResponse { response }.into())
                        .await?;
                }
                StateStatus::Software(SoftwareOperationVariants::List) => {
                    let response = SoftwareRequestResponse::new(&id, OperationStatus::Failed);
                    self.message_box
                        .send(SoftwareListResponse { response }.into())
                        .await?;
                }
                StateStatus::Restart(_) => {
                    error!("RestartOperation in store.");
                }
                StateStatus::UnknownOperation => {
                    error!("UnknownOperation in store.");
                }
            };
        }
        Ok(())
    }

    async fn handle_software_update_operation(
        &mut self,
        request: &SoftwareUpdateRequest,
        plugins: Arc<Mutex<ExternalPlugins>>,
    ) -> Result<(), SoftwareManagerError> {
        plugins.lock().await.load()?;
        plugins
            .lock()
            .await
            .update_default(&get_default_plugin(&self.config.config_location)?)?;

        self.state_repository
            .store(&State {
                operation_id: Some(request.id.clone()),
                operation: Some(StateStatus::Software(SoftwareOperationVariants::Update)),
            })
            .await?;

        // Send 'executing'
        let executing_response = SoftwareUpdateResponse::new(request);
        self.message_box.send(executing_response.into()).await?;

        let response = match self
            .operation_logs
            .new_log_file(LogKind::SoftwareUpdate)
            .await
        {
            Ok(log_file) => {
                plugins
                    .lock()
                    .await
                    .process(request, log_file, self.config.tmp_dir.as_std_path())
                    .await
            }
            Err(err) => {
                error!("{}", err);
                let mut failed_response = SoftwareUpdateResponse::new(request);
                failed_response.set_error(&format!("{}", err));
                failed_response
            }
        };
        self.message_box.send(response.into()).await?;

        let _state: State = self.state_repository.clear().await?;

        Ok(())
    }

    async fn handle_software_list_operation(
        &mut self,
        request: &SoftwareListRequest,
        plugins: Arc<Mutex<ExternalPlugins>>,
    ) -> Result<(), SoftwareManagerError> {
        self.state_repository
            .store(&State {
                operation_id: Some(request.id.clone()),
                operation: Some(StateStatus::Software(SoftwareOperationVariants::List)),
            })
            .await?;

        // Send 'executing'
        let executing_response = SoftwareListResponse::new(request);
        self.message_box.send(executing_response.into()).await?;

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
        self.message_box.send(response.into()).await?;

        let _state: State = self.state_repository.clear().await?;

        Ok(())
    }
}

fn get_default_plugin(
    config_location: &TEdgeConfigLocation,
) -> Result<Option<SoftwareType>, SoftwareManagerError> {
    let config_repository = tedge_config::TEdgeConfigRepository::new(config_location.clone());
    let tedge_config = config_repository.load()?;

    Ok(tedge_config.query_string_optional(SoftwarePluginDefaultSetting)?)
}
