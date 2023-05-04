use crate::software_list_manager::error::SoftwareListManagerError;
use crate::software_list_manager::error::SoftwareListManagerError::NoPlugins;
use crate::state_repository::state::AgentStateRepository;
use crate::state_repository::state::RestartOperationStatus;
use crate::state_repository::state::SoftwareOperationVariants;
use crate::state_repository::state::State;
use crate::state_repository::state::StateRepository;
use crate::state_repository::state::StateStatus;
use async_trait::async_trait;
use camino::Utf8PathBuf;
use plugin_sm::operation_logs::LogKind;
use plugin_sm::operation_logs::OperationLogs;
use plugin_sm::plugin_manager::ExternalPlugins;
use std::sync::Arc;
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
use tedge_api::SoftwareListRequest;
use tedge_api::SoftwareListResponse;
use tedge_api::SoftwareRequestResponse;
use tedge_api::SoftwareType;
use tedge_api::SoftwareUpdateResponse;
use tedge_config::system_services::SystemConfig;
use tedge_config::ConfigRepository;
use tedge_config::ConfigSettingAccessorStringExt;
use tedge_config::SoftwarePluginDefaultSetting;
use tedge_config::TEdgeConfigLocation;
use tokio::process::Command;
use tokio::sync::Mutex;
use tracing::error;
use tracing::info;
use tracing::log::warn;

const SYNC: &str = "sync";

#[cfg(not(test))]
const SUDO: &str = "sudo";
#[cfg(test)]
const SUDO: &str = "echo";

#[derive(Debug)]
pub struct SoftwareListManagerConfig {
    pub tmp_dir: Utf8PathBuf,
    pub tedge_root_path: Utf8PathBuf,
    pub system_config_path: Utf8PathBuf,
    pub sm_plugins_path: Utf8PathBuf,
    pub log_dir: Utf8PathBuf,
    pub default_plugin_type: Option<String>,
}

impl SoftwareListManagerConfig {
    pub fn new(
        tmp_dir: Utf8PathBuf,
        tedge_root_path: Utf8PathBuf,
        system_config_path: Utf8PathBuf,
        sm_plugins_path: Utf8PathBuf,
        log_dir: Utf8PathBuf,
        default_plugin_type: Option<String>,
    ) -> Self {
        Self {
            tmp_dir,
            tedge_root_path,
            system_config_path,
            sm_plugins_path,
            log_dir,
            default_plugin_type,
        }
    }
}

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
                &self.config.sm_plugins_path,
                self.config.default_plugin_type.clone(),
                Some(SUDO.into()),
            )
            .unwrap(),
        ));

        if plugins.lock().await.empty() {
            warn!(
                "{}",
                NoPlugins {
                    plugins_path: self.config.sm_plugins_path.clone(),
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
        // TODO: Modify AgentStateRepository to set different file name for each operation
        let state_repository = AgentStateRepository::new(config.tedge_root_path.clone());
        let operation_logs = OperationLogs::try_new(config.log_dir.clone().into()).unwrap();

        Self {
            config,
            state_repository,
            operation_logs,
            message_box,
        }
    }

    async fn process_pending_sm_list_operation(&mut self) -> Result<(), SoftwareListManagerError> {
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
                StateStatus::Software(SoftwareOperationVariants::List) => {
                    let response = SoftwareRequestResponse::new(&id, status);
                    self.message_box
                        .send(SoftwareListResponse { response })
                        .await?;
                }
                StateStatus::UnknownOperation => {
                    error!("UnknownOperation in store.");
                }
                _ => {
                    unimplemented!()
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
        let mut executing_response = SoftwareListResponse::new(&request);
        self.message_box.send(executing_response).await?;

        let response = match self
            .operation_logs
            .new_log_file(LogKind::SoftwareList)
            .await
        {
            Ok(log_file) => plugins.lock().await.list(&request, log_file).await,
            Err(err) => {
                error!("{}", err);
                let mut failed_response = SoftwareListResponse::new(&request);
                failed_response.set_error(&format!("{}", err));
                failed_response
            }
        };
        self.message_box.send(response).await?;

        let _state: State = self.state_repository.clear().await?;

        Ok(())
    }
}
