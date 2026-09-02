//! The connect step: establish (or stage) the cloud connection
//!
//! Built-in clouds reconnect via the existing bridge machinery;
//! custom mappers are their own bridge: their service is (re)started
//! and the connection verified via the mapper's health status.

use super::BootstrapCommand;
use crate::cli::bootstrap::cli::RegistrationMethod;
use crate::cli::reconnect::command::ReconnectBridgeCommand;
use crate::command::Command;
use crate::log::MaybeFancy;
use anyhow::anyhow;
use anyhow::Context;
use std::time::Duration;
use tedge_config::tedge_toml::ReadableKey;
use tedge_system_services::SystemService;

/// The first retry interval of a custom mapper's connection check
const CHECK_INITIAL_INTERVAL: Duration = Duration::from_secs(2);

/// The longest pause between two connection checks
const CHECK_MAX_INTERVAL: Duration = Duration::from_secs(30);

impl BootstrapCommand {
    /// Why offline service staging must defer, if anything.
    ///
    /// `tedge connect --offline` starts and enables the services and only
    /// skips the cloud checks - but building the bridge configuration
    /// itself hard-requires a device identity, and for basic auth the
    /// credentials file; without them the staging moves to the online run
    pub(super) async fn offline_staging_blocker(&self) -> Option<&'static str> {
        let Ok(config) = self.load_config().await else {
            return None; // a load failure surfaces properly in the step itself
        };
        let has_identity = self.device_id.is_some()
            || "device.id"
                .parse::<ReadableKey>()
                .ok()
                .and_then(|key| config.read_string(&key).ok())
                .is_some_and(|device_id| !device_id.is_empty());
        if !has_identity {
            return Some(
                "no device identity yet - provide --device-id \
                 to stage the services offline",
            );
        }
        if matches!(self.register, RegistrationMethod::Basic)
            && !self.registration_present(&config).await
        {
            return Some(
                "the basic-auth bridge configuration needs the credentials file; \
                 the services are staged by the online run",
            );
        }
        None
    }

    /// Establish the cloud connection (idempotent: disconnects first if needed)
    pub(super) async fn connect(&self) -> Result<(), MaybeFancy<anyhow::Error>> {
        if let Some(name) = self.custom_mapper_name() {
            return self.connect_custom_mapper(name).await;
        }
        // Offline staging mirrors `tedge connect --offline`:
        // all services are started and enabled, only the cloud checks
        // are skipped - they retry by themselves once the credentials
        // and network exist
        if self.offline {
            if let Some(reason) = self.offline_staging_blocker().await {
                self.ui
                    .line(&format!("offline: connection staging deferred ({reason})"));
                return Ok(());
            }
        }
        if self.dry_run {
            let profile = match self.profile() {
                Some(profile) => format!(" --profile {profile}"),
                None => String::new(),
            };
            let offline = if self.offline { " --offline" } else { "" };
            self.ui.line(&format!(
                "would run: tedge reconnect {}{profile}{offline}",
                self.cloud_name()
            ));
            return Ok(());
        }
        let reconnect = ReconnectBridgeCommand {
            config_dir: self.config_dir.clone(),
            cloud: self.cloud.clone(),
            offline_mode: self.offline,
            use_mapper: true,
            service_manager: self.service_manager.clone(),
        };
        let config = self.load_config().await?;
        if self.ui.is_verbose() {
            return reconnect.execute(config).await;
        }
        // The composed reconnect prints a dozen lines of its own;
        // capture them, summarize on success, dump in full on failure
        let log_path = std::env::temp_dir().join(format!(
            "tedge-bootstrap-connect-{}.log",
            std::process::id()
        ));
        let captured = match CapturedOutput::to_file(&log_path) {
            Ok(captured) => captured,
            // capturing is best-effort: fall back to the full stream
            Err(_) => return reconnect.execute(config).await,
        };
        let result = reconnect.execute(config).await;
        drop(captured);
        let output = std::fs::read_to_string(&log_path).unwrap_or_default();
        for line in output.lines() {
            self.ui.debug(line);
        }
        let _ = std::fs::remove_file(&log_path);
        if result.is_err() {
            self.ui
                .fail_line("connect failed; output of the connect step:");
            eprintln!("{output}");
        }
        result
    }

    /// Start and enable the custom mapper's service,
    /// then wait for it to report a healthy cloud connection
    async fn connect_custom_mapper(&self, name: &str) -> Result<(), MaybeFancy<anyhow::Error>> {
        let service_name = format!("tedge-mapper-{name}");
        if self.dry_run {
            self.ui.line(&format!(
                "would restart and enable the {service_name} service \
                 and wait for it to connect"
            ));
            return Ok(());
        }
        self.ui.line(&format!(
            "restarting and enabling the {service_name} service"
        ));
        let service = SystemService {
            name: &service_name,
            profile: None,
        };
        self.service_manager
            .restart_service(service)
            .await
            .with_context(|| {
                format!(
                    "Failed to restart {service_name}; \
                     is the {name} mapper package installed?"
                )
            })?;
        self.service_manager
            .enable_service(service)
            .await
            .map_err(anyhow::Error::new)?;

        if self.offline {
            self.ui.line(
                "offline: connection verification deferred - \
                 the mapper connects when the network is available",
            );
            return Ok(());
        }

        // Reuse the existing connection test, which waits for the mapper
        // to report a healthy cloud connection.
        //
        // The test itself fails fast, but the first connection can be slow
        // (service start, DNS, TLS) or depend on an operator action
        // (e.g. registering a certificate in the cloud's UI),
        // so it is retried until the connect timeout:
        // exponential backoff like the built-in clouds' connection check,
        // but bounded by a time budget rather than an attempt count
        let mut interval = CHECK_INITIAL_INTERVAL;
        let mut attempt = 0;
        let deadline = self
            .connect_timeout
            .map(|timeout| tokio::time::Instant::now() + timeout);
        loop {
            attempt += 1;
            let config = self.load_config().await?;
            let error =
                match crate::cli::connect::wait_for_custom_mapper_health(&config, name).await {
                    Ok(()) => return Ok(()),
                    Err(error) => anyhow!("{error}"),
                };
            let Some(deadline) = deadline else {
                return Err(error.into());
            };
            if tokio::time::Instant::now() >= deadline {
                return Err(anyhow!(
                    "The {service_name} service did not report a healthy connection \
                     within {}s (last error: {error}). \
                     Registration is kept: fix the cause (e.g. complete \
                     any pending step in the cloud's UI) and re-run \
                     `tedge bootstrap {}` to retry the connection only",
                    self.connect_timeout.unwrap_or_default().as_secs(),
                    self.cloud_name(),
                )
                .into());
            }
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let pause = interval.min(remaining);
            self.ui.line(&format!(
                "connection test failed, attempt {attempt}; retrying in {}s \
                 (Ctrl-C to abort; re-running resumes without re-registering)",
                pause.as_secs()
            ));
            tokio::time::sleep(pause).await;
            interval = (interval * 2).min(CHECK_MAX_INTERVAL);
        }
    }
}

/// Process-wide stdout/stderr redirection to a file, restored on drop:
/// used to quiet a composed command's output, keeping it for failures.
///
/// This is a process-wide side effect: anything else writing to the
/// standard streams while it is in place (tracing, for instance) lands
/// in the file too — acceptable for the sequential connect step,
/// whose output is then replayed into the bootstrap log anyway
struct CapturedOutput {
    saved_stdout: std::os::fd::OwnedFd,
    saved_stderr: std::os::fd::OwnedFd,
}

impl CapturedOutput {
    fn to_file(path: &std::path::Path) -> anyhow::Result<Self> {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        // private to the user running bootstrap: the file lives in a
        // shared temp directory
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        std::io::stdout().flush().ok();
        std::io::stderr().flush().ok();
        let saved_stdout = nix::unistd::dup(std::io::stdout())?;
        let saved_stderr = nix::unistd::dup(std::io::stderr())?;
        nix::unistd::dup2_stdout(&file)?;
        nix::unistd::dup2_stderr(&file)?;
        Ok(Self {
            saved_stdout,
            saved_stderr,
        })
    }
}

impl Drop for CapturedOutput {
    fn drop(&mut self) {
        use std::io::Write;
        std::io::stdout().flush().ok();
        std::io::stderr().flush().ok();
        let _ = nix::unistd::dup2_stdout(&self.saved_stdout);
        let _ = nix::unistd::dup2_stderr(&self.saved_stderr);
    }
}
