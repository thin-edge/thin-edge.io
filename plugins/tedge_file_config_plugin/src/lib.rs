pub mod bin;
pub mod config;
mod error;

use crate::config::FileEntry;
pub use crate::config::PluginConfig;
use crate::config::TedgeWriteStatus;
use crate::error::PluginError;
use anyhow::Context;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use log::debug;
use log::info;
use std::fs::File;
use std::io::stdout;
use std::io::BufReader;
use std::io::ErrorKind;
use tedge_system_services::GeneralServiceManager;
use tedge_system_services::SystemService;
use tedge_system_services::SystemServiceManager;
use tedge_utils::atomic::MaybePermissions;
use tedge_write::CopyOptions;
use tedge_write::CreateDirsOptions;

pub struct FileConfigPlugin {
    config: PluginConfig,
    use_tedge_write: TedgeWriteStatus,
    service_manager: GeneralServiceManager,
}

impl FileConfigPlugin {
    pub fn new(
        config: PluginConfig,
        use_tedge_write: TedgeWriteStatus,
        service_manager: GeneralServiceManager,
    ) -> Self {
        Self {
            config,
            use_tedge_write,
            service_manager,
        }
    }

    /// List all configuration types supported by this plugin
    pub fn list(&self) -> Result<Vec<String>, PluginError> {
        Ok(self.config.get_all_file_types())
    }

    /// Get the configuration for a specific type
    /// Reads the file and returns its contents
    pub fn get(&self, config_type: &str) -> Result<(), PluginError> {
        let entry = self
            .config
            .get_file_entry(config_type)
            .ok_or_else(|| PluginError::InvalidConfigType(config_type.to_string()))?;

        let config_path = Utf8PathBuf::from(&entry.path);
        if !config_path.exists() {
            return Err(PluginError::FileNotFound(config_path));
        }

        let file =
            File::open(&config_path).with_context(|| format!("failed to read: '{config_path}'"))?;
        let mut reader = BufReader::new(&file);
        let mut out = stdout().lock();

        std::io::copy(&mut reader, &mut out)
            .with_context(|| format!("failed to print contents of: '{config_path}' to stdout"))?;
        Ok(())
    }

    /// Set the configuration for a specific type
    /// Deploys the new config from source_path to the configured destination
    pub async fn set(&self, config_type: &str, from: &Utf8Path) -> Result<(), PluginError> {
        let entry = self
            .config
            .get_file_entry(config_type)
            .ok_or_else(|| PluginError::InvalidConfigType(config_type.to_string()))?;

        let to = Utf8PathBuf::from(&entry.path);

        // Create parent directory if it doesn't exist
        if let Some(parent) = to.parent() {
            if !parent.exists() {
                self.create_parent_dirs(parent, entry)?;
            }
        }

        // Deploy the config file
        self.deploy_config_file(from, entry)?;

        Ok(())
    }

    async fn execute_service_action(&self, service_name: &str, action: &str) -> anyhow::Result<()> {
        // Create a service for the arbitrary service name
        let service = SystemService::new(service_name);

        info!("Executing: {action} on service: {service_name}");

        match action {
            "restart" => {
                self.service_manager
                    .restart_service(service)
                    .await
                    .with_context(|| {
                        format!("Failed to run restart command for the service: {service_name}")
                    })?;
            }
            _ => {
                anyhow::bail!(
                    "Unsupported service action: {action}. Only 'restart' is currently supported in the file config plugin context."
                );
            }
        }

        info!("Successfully executed: {action} on service: {service_name}");

        Ok(())
    }

    /// Prepare for configuration update
    pub async fn prepare(
        &self,
        _config_type: &str,
        _from_path: &Utf8Path,
        _workdir: &Utf8Path,
    ) -> Result<(), PluginError> {
        // No-op for backward compatibility
        Ok(())
    }

    /// Apply configuration by restarting service if configured
    pub async fn apply(&self, config_type: &str, _workdir: &Utf8Path) -> Result<(), PluginError> {
        let entry = self
            .config
            .get_file_entry(config_type)
            .ok_or_else(|| PluginError::InvalidConfigType(config_type.to_string()))?;

        // Execute service action if defined for the config type
        if let Some(service_name) = &entry.service {
            let action = entry
                .service_action
                .as_deref()
                .expect("service_action must be set when service is set");

            self.execute_service_action(service_name, action)
                .await
                .with_context(|| format!("Failed to {action} service: {service_name}"))?;
        }

        Ok(())
    }

    /// Verify configuration was applied successfully
    pub async fn verify(&self, config_type: &str, _workdir: &Utf8Path) -> Result<(), PluginError> {
        let entry = self
            .config
            .get_file_entry(config_type)
            .ok_or_else(|| PluginError::InvalidConfigType(config_type.to_string()))?;

        // If service is configured, verify it's running
        if let Some(service_name) = &entry.service {
            let service = SystemService::new(service_name);

            let is_running = self
                .service_manager
                .is_service_running(service)
                .await
                .with_context(|| format!("Failed to check if service {service_name} is running"))?;

            if !is_running {
                return Err(PluginError::ServiceNotRunning(service_name.to_string()));
            }
        }

        Ok(())
    }

    /// Finalize configuration update
    pub async fn finalize(
        &self,
        config_type: &str,
        _workdir: &Utf8Path,
    ) -> Result<(), PluginError> {
        info!("Configuration update finalized successfully for {config_type}");
        // No-op due for backward compatibility
        Ok(())
    }

    /// Rollback configuration to previous state
    pub async fn rollback(
        &self,
        _config_type: &str,
        _workdir: &Utf8Path,
    ) -> Result<(), PluginError> {
        // No-op due for backward compatibility
        Ok(())
    }

    /// Creates the parent directories of the target file if they are missing,
    /// and applies the permissions and ownership that are specified.
    /// First, if `use_tedge_write` is enabled, it tries to use tedge-write to create the missing parent directories.
    /// If it's disabled or creation with elevated privileges fails, fall back to the current user.
    fn create_parent_dirs(&self, parent: &Utf8Path, file_entry: &FileEntry) -> anyhow::Result<()> {
        if let TedgeWriteStatus::Enabled { sudo } = self.use_tedge_write.clone() {
            debug!("Creating the missing parent directories with elevation at '{parent}'");
            let result = CreateDirsOptions {
                dir_path: parent,
                sudo,
                mode: file_entry.parent_permissions.mode,
                user: file_entry.parent_permissions.user.as_deref(),
                group: file_entry.parent_permissions.group.as_deref(),
            }
            .create();

            match result {
                Ok(()) => return Ok(()),
                Err(err) => {
                    info!("Failed to create the missing parent directories with elevation at '{parent}' with error: {err}. \
            Falling back to the current user to create the directories.");
                }
            }
        }

        debug!("Creating the missing parent directories without elevation at '{parent}'");
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create parent directories. Path: '{parent}'"))?;

        file_entry.parent_permissions.clone().apply_sync(parent.as_std_path())
            .with_context(|| format!("failed to change permissions or mode of the parent directory. Path: '{parent}'"))?;

        Ok(())
    }

    /// Deploys the new version of the configuration file and returns the path under which it was
    /// deployed.
    ///
    /// Ensures that the configuration file under `dest` is overwritten atomically by a new version
    /// currently stored in a temporary directory.
    ///
    /// If the configuration file doesn't already exist, a new file with target permissions is
    /// created. If the configuration file already exists, its content is overwritten, but owner and
    /// mode remains unchanged.
    ///
    /// If `use_tedge_write` is enabled, a `tedge-write` process is spawned when privilege elevation
    /// is required.
    fn deploy_config_file(
        &self,
        from: &Utf8Path,
        file_entry: &FileEntry,
    ) -> anyhow::Result<Utf8PathBuf> {
        let to = Utf8PathBuf::from(&file_entry.path);
        let permissions = MaybePermissions::try_from(&file_entry.file_permissions)?;

        let src = std::fs::File::open(from)
            .with_context(|| format!("failed to open source temporary file '{from}'"))?;

        let Err(err) = tedge_utils::atomic::write_file_atomic_set_permissions_if_doesnt_exist(
            src,
            &to,
            &permissions,
        )
        .with_context(|| format!("failed to deploy config file from '{from}' to '{to}'")) else {
            return Ok(to);
        };

        if let Some(io_error) = err.downcast_ref::<std::io::Error>() {
            if io_error.kind() != ErrorKind::PermissionDenied {
                return Err(err);
            }
        }

        match self.use_tedge_write.clone() {
            TedgeWriteStatus::Disabled => {
                return Err(err);
            }

            TedgeWriteStatus::Enabled { sudo } => {
                let mode = file_entry.file_permissions.mode;
                let user = file_entry.file_permissions.user.as_deref();
                let group = file_entry.file_permissions.group.as_deref();

                let options = CopyOptions {
                    from,
                    to: to.as_path(),
                    sudo,
                    mode,
                    user,
                    group,
                };

                options.copy()?;
            }
        }

        Ok(to)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tedge_test_utils::fs::TempTedgeDir;

    #[test]
    fn test_list_returns_all_types() {
        let ttd = TempTedgeDir::new();
        let toml_content = r#"
[[files]]
path = "/etc/app.conf"
type = "app"

[[files]]
path = "/etc/service.toml"
type = "service"
"#;
        let config_file = ttd
            .file("plugin_config.toml")
            .with_raw_content(toml_content);
        let config = PluginConfig::new(config_file.utf8_path());

        let service_manager = GeneralServiceManager::try_new(ttd.utf8_path()).unwrap();
        let plugin = FileConfigPlugin::new(config, TedgeWriteStatus::Disabled, service_manager);
        let types = plugin.list().unwrap();

        assert_eq!(types.len(), 3); // Includes the plugin config itself
        assert!(types.contains(&"app".to_string()));
        assert!(types.contains(&"service".to_string()));
        assert!(types.contains(&"tedge-configuration-plugin".to_string()));
    }

    #[test]
    fn test_list_returns_default_type_for_empty_file() {
        let ttd = TempTedgeDir::new();
        let config_file = ttd.file("plugin_config.toml").with_raw_content("");
        let config = PluginConfig::new(config_file.utf8_path());

        let service_manager = GeneralServiceManager::try_new(ttd.utf8_path()).unwrap();
        let plugin = FileConfigPlugin::new(config, TedgeWriteStatus::Disabled, service_manager);
        let types = plugin.list().unwrap();

        assert_eq!(types.len(), 1); // Only the plugin config itself
        assert!(types.contains(&"tedge-configuration-plugin".to_string()));
    }

    #[test]
    fn test_list_returns_default_type_for_non_existent_file() {
        let ttd = TempTedgeDir::new();
        let config = PluginConfig::new(&ttd.utf8_path().join("no_file.toml"));

        let service_manager = GeneralServiceManager::try_new(ttd.utf8_path()).unwrap();
        let plugin = FileConfigPlugin::new(config, TedgeWriteStatus::Disabled, service_manager);
        let types = plugin.list().unwrap();

        assert_eq!(types.len(), 1); // Only the plugin config itself
        assert!(types.contains(&"tedge-configuration-plugin".to_string()));
    }

    #[test]
    fn test_get_unsupported_type() {
        let ttd = TempTedgeDir::new();
        let toml_content = r#"
[[files]]
path = "/etc/app.conf"
type = "app.conf"
"#;
        let config_file = ttd
            .file("plugin_config.toml")
            .with_raw_content(toml_content);
        let config = PluginConfig::new(config_file.utf8_path());

        let service_manager = GeneralServiceManager::try_new(ttd.utf8_path()).unwrap();
        let plugin = FileConfigPlugin::new(config, TedgeWriteStatus::Disabled, service_manager);
        let result = plugin.get("unknown`");

        assert!(matches!(result, Err(PluginError::InvalidConfigType(_))));
    }

    #[test]
    fn test_get_file_not_found() {
        let ttd = TempTedgeDir::new();
        let non_existent_path = ttd.path().join("does-not-exist.conf");

        let toml_content = format!(
            r#"
[[files]]
path = "{}"
type = "missing.conf"
"#,
            non_existent_path.to_str().unwrap()
        );
        let config_file = ttd
            .file("plugin_config.toml")
            .with_raw_content(&toml_content);
        let config = PluginConfig::new(config_file.utf8_path());

        let service_manager = GeneralServiceManager::try_new(ttd.utf8_path()).unwrap();
        let plugin = FileConfigPlugin::new(config, TedgeWriteStatus::Disabled, service_manager);
        let result = plugin.get("missing.conf");

        assert!(matches!(result, Err(PluginError::FileNotFound(_))));
    }

    #[tokio::test]
    async fn test_set_unsupported_type() {
        let ttd = TempTedgeDir::new();
        let source_file = ttd.file("source.conf").with_raw_content("test content");

        let toml_content = r#"
[[files]]
path = "/etc/app.conf"
type = "app.conf"
"#;
        let config_file = ttd
            .file("plugin_config.toml")
            .with_raw_content(toml_content);
        let config = PluginConfig::new(config_file.utf8_path());

        let service_manager = GeneralServiceManager::try_new(ttd.utf8_path()).unwrap();
        let plugin = FileConfigPlugin::new(config, TedgeWriteStatus::Disabled, service_manager);
        let result = plugin
            .set("unknown.conf", source_file.path().try_into().unwrap())
            .await;

        assert!(matches!(result, Err(PluginError::InvalidConfigType(_))));
    }

    #[tokio::test]
    async fn test_set_overwrites_existing_file() {
        let ttd = TempTedgeDir::new();

        // Create destination file with original content
        let dest_file_path = ttd.path().join("dest.conf");
        ttd.file("dest.conf").with_raw_content("original=content\n");

        // Create source file with new content
        let new_content = "updated=content\n";
        let source_file = ttd.file("source.conf").with_raw_content(new_content);

        let toml_content = format!(
            r#"
[[files]]
path = "{}"
type = "test.conf"
"#,
            dest_file_path.to_str().unwrap()
        );
        let config_file = ttd
            .file("plugin_config.toml")
            .with_raw_content(&toml_content);
        let config = PluginConfig::new(config_file.utf8_path());

        let service_manager = GeneralServiceManager::try_new(ttd.utf8_path()).unwrap();
        let plugin = FileConfigPlugin::new(config, TedgeWriteStatus::Disabled, service_manager);
        let result = plugin
            .set("test.conf", source_file.path().try_into().unwrap())
            .await;

        assert!(result.is_ok());

        let actual_content = std::fs::read_to_string(&dest_file_path).unwrap();
        assert_eq!(actual_content, new_content);
    }
}
