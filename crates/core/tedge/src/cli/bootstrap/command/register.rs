//! The register step: obtain device credentials
//!
//! Either a built-in Cumulocity method (see [`crate::cli::bootstrap::c8y`]),
//! or the register.d hooks; the outcome of the hooks is verified
//! by the presence of the credential artifacts.

use super::BootstrapCommand;
use crate::cli::bootstrap::hooks;
use crate::cli::bootstrap::hooks::HookContext;
use crate::cli::bootstrap::hooks::Phase;
use crate::cli::bootstrap::resolve::RegistrationMethod;
use crate::log::MaybeFancy;
use anyhow::anyhow;
use tedge_config::TEdgeConfig;

impl BootstrapCommand {
    /// Obtain device credentials using the selected registration method
    pub(super) async fn register(
        &self,
        hook_ctx: &HookContext<'_>,
    ) -> Result<(), MaybeFancy<anyhow::Error>> {
        let config = self.load_config().await?;
        match self.register {
            RegistrationMethod::Hook { .. } => self.register_via_hooks(&config, hook_ctx).await,
            // The built-in methods are Cumulocity's (enforced when resolving the method)
            _ => self.register_builtin(config).await,
        }
    }

    /// Delegate registration to the register.d hooks and verify the outcome
    async fn register_via_hooks(
        &self,
        config: &TEdgeConfig,
        hook_ctx: &HookContext<'_>,
    ) -> Result<(), MaybeFancy<anyhow::Error>> {
        // Method names are cloud vocabulary: make the scoping visible in the output
        if let Some(method) = &hook_ctx.register_method {
            self.detail(&format!(
                "using the {} \"{method}\" method",
                self.cloud_name()
            ));
        }
        let hooks_run = hooks::run_phase(Phase::Register, hook_ctx).await?;
        if hooks_run == 0 {
            let dirs = hooks::phase_dirs(Phase::Register, &self.plugin_paths);
            let dirs = dirs
                .iter()
                .map(|dir| dir.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            if self.dry_run {
                self.ui.line(&format!(
                    "note: registration requires register hooks, \
                     but none are currently installed in: {dirs}"
                ));
                return Ok(());
            }
            if self.offline {
                self.ui.line(
                    "offline: registration deferred (no register hooks installed) - \
                     re-run this command once the device is online",
                );
                return Ok(());
            }
            return Err(anyhow!(
                "--register hook requires at least one register hook, \
                 but none were found in: {dirs}"
            )
            .into());
        }
        if self.dry_run {
            return Ok(());
        }

        // Verify the hooks produced the credential artifacts before connecting.
        // The connect step remains the end-to-end proof.
        let candidates = self.registration_artifacts(config).await?;
        if !candidates.iter().any(|path| path.exists()) {
            // Offline, hooks that self-skip are the expected outcome:
            // registration is deferred, not failed
            if self.offline {
                self.ui.line(
                    "offline: registration deferred (no register hook produced \
                     credentials) - re-run this command once the device is online",
                );
                return Ok(());
            }
            let paths = candidates
                .iter()
                .map(|path| path.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(anyhow!(
                "The register hooks did not produce device credentials; \
                 expected one of: {paths}"
            )
            .into());
        }
        Ok(())
    }
}
