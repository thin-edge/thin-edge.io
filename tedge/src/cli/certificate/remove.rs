use crate::command::{Command, ExecutionContext};
use crate::utils::{paths::*, users::*};

use tedge_config::*;

use super::error::CertError;

/// Remove the device certificate
pub struct RemoveCertCmd {
    /// The path of the certificate to be removed
    pub cert_path: FilePath,

    /// The path of the private key to be removed
    pub key_path: FilePath,
}

impl Command for RemoveCertCmd {
    fn description(&self) -> String {
        "remove the device certificate".into()
    }

    fn execute(&self, context: &ExecutionContext) -> Result<(), anyhow::Error> {
        let () = self.remove_certificate(&context.user_manager)?;
        Ok(())
    }
}

impl RemoveCertCmd {
    fn remove_certificate(&self, user_manager: &UserManager) -> Result<(), CertError> {
        let _user_guard = user_manager.become_user(crate::utils::users::BROKER_USER)?;
        std::fs::remove_file(&self.cert_path).or_else(ok_if_not_found)?;
        std::fs::remove_file(&self.key_path).or_else(ok_if_not_found)?;

        Ok(())
    }
}
