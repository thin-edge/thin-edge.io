use crate::cli::connect::*;
use crate::services::{
    self, mosquitto::MosquittoService, tedge_mapper::TedgeMapperService, SystemdService,
};
use crate::utils::paths;
use crate::utils::users::UserManager;
use std::path::Path;
use tempfile::NamedTempFile;
use url::Url;
use which::which;

impl BridgeConfig {
    pub fn new_bridge(&self, user_manager: &UserManager) -> Result<(), ConnectError> {
        println!("Checking if systemd is available.\n");
        let _ = services::systemd_available()?;

        println!("Checking if configuration for requested bridge already exists.\n");
        let _ = self.bridge_config_exists()?;

        println!("Validating the bridge certificates.\n");
        self.validate()?;

        println!("Saving configuration for requested bridge.\n");
        if let Err(err) = self.write_bridge_config_to_file() {
            // We want to preserve previous errors and therefore discard result of this function.
            let _ = self.clean_up();
            return Err(err);
        }

        println!("Restarting mosquitto service.\n");
        if let Err(err) = MosquittoService.restart(user_manager) {
            self.clean_up()?;
            return Err(err.into());
        }

        println!(
            "Awaiting mosquitto to start. This may take up to {} seconds.\n",
            MOSQUITTO_RESTART_TIMEOUT_SECONDS
        );
        std::thread::sleep(std::time::Duration::from_secs(
            MOSQUITTO_RESTART_TIMEOUT_SECONDS,
        ));

        println!("Persisting mosquitto on reboot.\n");
        if let Err(err) = MosquittoService.enable(user_manager) {
            self.clean_up()?;
            return Err(err.into());
        }

        println!("Successfully created bridge connection!\n");

        if self.use_mapper {
            println!("Checking if tedge-mapper is installed.\n");

            if which("tedge_mapper").is_err() {
                println!("Warning: tedge_mapper is not installed. We recommend to install it.\n");
            } else {
                self.start_and_enable_tedge_mapper(user_manager);
            }
        }

        Ok(())
    }

    // To preserve error chain and not discard other errors we need to ignore error here
    // (don't use '?' with the call to this function to preserve original error).
    fn clean_up(&self) -> Result<(), ConnectError> {
        let path = self.get_bridge_config_file_path()?;
        let _ = std::fs::remove_file(&path).or_else(ok_if_not_found)?;
        Ok(())
    }

    fn bridge_config_exists(&self) -> Result<(), ConnectError> {
        let path = self.get_bridge_config_file_path()?;
        if Path::new(&path).exists() {
            return Err(ConnectError::ConfigurationExists {
                cloud: self.cloud_name.to_string(),
            });
        }
        Ok(())
    }

    fn write_bridge_config_to_file(&self) -> Result<(), ConnectError> {
        let dir_path = paths::build_path_for_sudo_or_user(&[TEDGE_BRIDGE_CONF_DIR_PATH])?;

        // This will forcefully create directory structure if it doesn't exist, we should find better way to do it, maybe config should deal with it?
        let _ = paths::create_directories(&dir_path)?;

        let mut common_temp_file = NamedTempFile::new()?;
        self.common_mosquitto_config
            .serialize(&mut common_temp_file)?;
        let common_config_path = self.get_common_mosquitto_config_file_path()?;
        let _ = paths::persist_tempfile(common_temp_file, &common_config_path)?;

        let mut temp_file = NamedTempFile::new()?;
        self.serialize(&mut temp_file)?;
        let config_path = self.get_bridge_config_file_path()?;
        let _ = paths::persist_tempfile(temp_file, &config_path)?;

        Ok(())
    }

    pub fn validate(&self) -> Result<(), ConnectError> {
        Url::parse(&self.address)?;

        if !self.bridge_root_cert_path.as_ref().exists() {
            return Err(ConnectError::Certificate);
        }

        if !self.bridge_certfile.as_ref().exists() {
            return Err(ConnectError::Certificate);
        }

        if !self.bridge_keyfile.as_ref().exists() {
            return Err(ConnectError::Certificate);
        }

        Ok(())
    }

    fn get_bridge_config_file_path(&self) -> Result<String, ConnectError> {
        Ok(paths::build_path_for_sudo_or_user(&[
            TEDGE_BRIDGE_CONF_DIR_PATH,
            &self.config_file,
        ])?)
    }

    fn start_and_enable_tedge_mapper(&self, user_manager: &UserManager) {
        let mut failed = false;

        println!("Starting tedge-mapper service.\n");
        if let Err(err) = TedgeMapperService.restart(user_manager) {
            println!("Failed to stop tedge-mapper service: {:?}", err);
            failed = true;
        }

        println!("Persisting tedge-mapper on reboot.\n");
        if let Err(err) = TedgeMapperService.enable(user_manager) {
            println!("Failed to enable tedge-mapper service: {:?}", err);
            failed = true;
        }

        if !failed {
            println!("tedge-mapper service successfully started and enabled!\n");
        }
    }

    fn get_common_mosquitto_config_file_path(&self) -> Result<String, ConnectError> {
        Ok(paths::build_path_for_sudo_or_user(&[
            TEDGE_BRIDGE_CONF_DIR_PATH,
            &self.common_mosquitto_config.config_file,
        ])?)
    }
}

fn ok_if_not_found(err: std::io::Error) -> std::io::Result<()> {
    match err.kind() {
        std::io::ErrorKind::NotFound => Ok(()),
        _ => Err(err),
    }
}
