//! Test fixtures shared by the pipeline step tests

use super::BootstrapCommand;
use super::OneTimePassword;
use crate::cli::bootstrap::invocation::Invocation;
use crate::cli::bootstrap::resolve::RegistrationMethod;
use crate::cli::bootstrap::ui::Ui;
use crate::cli::common::Cloud;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use tedge_config::TEdgeConfig;
use tedge_system_services::ServiceCommandOutcome;
use tedge_system_services::SystemService;
use tedge_system_services::SystemServiceError;
use tedge_system_services::SystemServiceManager;

/// Records the service operations of a run instead of performing them
#[derive(Debug, Default)]
pub struct StubServiceManager {
    pub calls: Mutex<Vec<String>>,
}

#[async_trait::async_trait]
impl SystemServiceManager for StubServiceManager {
    fn name(&self) -> &str {
        "stub"
    }
    async fn check_operational(&self) -> Result<(), SystemServiceError> {
        Ok(())
    }
    /// Every service operation goes through here: recorded, always successful
    async fn run_action(
        &self,
        action: &str,
        service: SystemService<'_>,
    ) -> Result<ServiceCommandOutcome, SystemServiceError> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("{action} {}", service.name));
        Ok(ServiceCommandOutcome {
            service_command: format!("{action} {}", service.name),
            status: std::process::ExitStatus::default(),
            stdout: String::new(),
            stderr: String::new(),
        })
    }
    async fn is_service_running(
        &self,
        _service: SystemService<'_>,
    ) -> Result<bool, SystemServiceError> {
        Ok(false)
    }
}

/// A device config directory with a tedge.toml, and commands against it
pub struct Fixture {
    _tmp: tempfile::TempDir,
    pub config_dir: Utf8PathBuf,
    pub services: Arc<StubServiceManager>,
}

impl Fixture {
    pub async fn new(tedge_toml: &str) -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let config_dir = Utf8PathBuf::try_from(tmp.path().to_owned()).unwrap();
        tokio::fs::write(config_dir.join("tedge.toml"), tedge_toml)
            .await
            .unwrap();
        Self {
            _tmp: tmp,
            config_dir,
            services: Arc::new(StubServiceManager::default()),
        }
    }

    pub async fn config(&self) -> TEdgeConfig {
        TEdgeConfig::load(&self.config_dir).await.unwrap()
    }

    pub fn command(&self, cloud: Cloud) -> BootstrapCommand {
        BootstrapCommand {
            config_dir: self.config_dir.clone(),
            plugin_paths: Vec::new(),
            service_manager: self.services.clone(),
            invocation: Invocation {
                cloud: cloud.short_name().to_owned(),
                ..Default::default()
            },
            cloud,
            cloud_type: None,
            url: None,
            register: RegistrationMethod::C8yCa,
            device_id: None,
            one_time_password: OneTimePassword::None,
            settings: Vec::new(),
            method_settings: Vec::new(),
            connect_timeout: None,
            hook_envs: Vec::new(),
            re_register: false,
            clean: false,
            offline: false,
            ui: Arc::new(Ui::new(Some(self.config_dir.clone().into()), true)),
            dry_run: false,
        }
    }
}

pub fn touch(path: &Utf8Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, "content").unwrap();
}
