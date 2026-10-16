//! The Cumulocity-specific parts of the bootstrap pipeline
//!
//! Cumulocity is the only cloud with built-in registration methods
//! (`c8y-ca`, `self-signed`, `basic`, `basic-preregistered`),
//! an MQTT endpoint discovered from the HTTP one,
//! and per-instance defaults (topic prefix, proxy port, certificate paths)
//! keeping several instances from clashing.
//! These apply to the built-in `c8y` cloud, its profiles,
//! and custom-named instances typed `c8y`.

mod basic;
mod configure;
mod register;

use super::command::configure::normalize_http_url;
use super::command::configure::url_host;
use super::command::BootstrapCommand;
use super::mapper_toml::instance_cert_path;
use super::mapper_toml::MapperToml;
use super::resolve::RegistrationMethod;
use super::settings::key;
use anyhow::anyhow;
use anyhow::Context;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use std::time::Duration;
use tedge_config::models::HostPort;
use tedge_config::models::HTTPS_PORT;
use tedge_config::tedge_toml::mapper_config::C8yMapperSpecificConfig;
use tedge_config::tedge_toml::mapper_config::MapperConfig;
use tedge_config::TEdgeConfig;

/// The cloud name, as a cloud descriptor key and hook argument
pub const CLOUD: &str = "c8y";

/// The built-in registration methods, as cloud vocabulary
pub mod method {
    pub const CA: &str = "c8y-ca";
    pub const SELF_SIGNED: &str = "self-signed";
    pub const BASIC: &str = "basic";
    pub const BASIC_PREREGISTERED: &str = "basic-preregistered";
}

/// The environment variables of the built-in methods' inputs
/// (as declared by the compiled-in descriptor) and of the run context
pub mod env {
    /// The c8y-ca one-time password, shared with `tedge cert download c8y`
    pub const ONE_TIME_PASSWORD: &str = "DEVICE_ONE_TIME_PASSWORD";
    /// The self-signed method's user credentials, shared with `tedge cert upload c8y`
    pub const USER: &str = "C8Y_USER";
    pub const PASSWORD: &str = "C8Y_PASSWORD";
    pub const PASSWORD_DEPRECATED: &str = "C8YPASS";
    /// The basic method's inputs
    pub const BOOTSTRAP_USER: &str = "C8Y_BOOTSTRAP_USER";
    pub const BOOTSTRAP_PASSWORD: &str = "C8Y_BOOTSTRAP_PASSWORD";
    pub const SECURITY_TOKEN: &str = "C8Y_SECURITY_TOKEN";
    /// The basic-preregistered method's inputs
    pub const DEVICE_USER: &str = "C8Y_DEVICE_USER";
    pub const DEVICE_PASSWORD: &str = "C8Y_DEVICE_PASSWORD";
    /// Exported to hooks: the pending c8y-ca registration URL
    pub const REGISTRATION_URL: &str = "C8Y_REGISTRATION_URL";
}

/// The local proxy port of the default Cumulocity instance
pub const DEFAULT_PROXY_PORT: u16 = 8001;

/// How long to wait for an operator to accept a pending registration
const REGISTRATION_TIMEOUT: Duration = Duration::from_secs(600);

impl BootstrapCommand {
    /// Whether this instance speaks Cumulocity
    /// (the built-in cloud, or a custom-named instance typed c8y)
    pub(super) fn is_c8y(&self) -> bool {
        self.cloud_name() == CLOUD || self.cloud_type.as_deref() == Some(CLOUD)
    }

    /// The Cumulocity mapper configuration of a built-in instance
    /// (the default instance or a profile)
    pub(super) fn c8y_config(
        &self,
        config: &TEdgeConfig,
    ) -> anyhow::Result<MapperConfig<C8yMapperSpecificConfig>> {
        config.mapper_config(&self.profile().cloned())
    }

    /// The Cumulocity HTTP host this run registers against:
    /// the given URL, else the instance's configured endpoint
    pub(super) async fn c8y_http_host(&self, config: &TEdgeConfig) -> Option<HostPort<HTTPS_PORT>> {
        if let Some(url) = &self.url {
            let host = url_host(&normalize_http_url(url)).ok()?;
            return HostPort::try_from(host.as_str()).ok();
        }
        match self.custom_mapper_name() {
            Some(name) => self.named_instance_c8y_url(name).await.ok(),
            None => self
                .c8y_config(config)
                .ok()?
                .cloud_specific
                .http
                .or_config_not_set()
                .ok()
                .cloned(),
        }
    }

    /// The Cumulocity URL of a custom-named c8y instance, from its mapper.toml
    pub(super) async fn named_instance_c8y_url(
        &self,
        name: &str,
    ) -> anyhow::Result<HostPort<HTTPS_PORT>> {
        let path = self.mapper_config_path(name);
        let mapper_toml = MapperToml::load(&path).await?;
        let url = mapper_toml
            .url()
            .with_context(|| format!("The {name} instance has no URL configured; pass --url"))?;
        HostPort::try_from(url).map_err(|e| anyhow!("Invalid URL {url:?} in {path}: {e}"))
    }

    /// The device certificate of this instance for the certificate methods:
    /// a custom-named instance's own, else the (profile's) configured path
    pub(super) fn c8y_cert_path(&self, config: &TEdgeConfig) -> Option<Utf8PathBuf> {
        match self.custom_mapper_name() {
            Some(name) => Some(instance_cert_path(&self.mapper_dir(name))),
            None => config
                .device_cert_path(Some(&self.cloud))
                .ok()
                .map(Into::into),
        }
    }

    /// The Cumulocity device-registration URL of the registration this run
    /// is about to perform: c8y-ca, with a known device id and host,
    /// and not skipped (no certificate yet, or a --re-register/--clean run).
    /// The URL is pre-filled with the pre-generated one-time password
    pub(super) async fn pending_registration_url(&self, config: &TEdgeConfig) -> Option<String> {
        if !matches!(self.register, RegistrationMethod::C8yCa) {
            return None;
        }
        // a password supplied by the user is kept secret
        let password = self.one_time_password.generated()?;
        let registered = self.c8y_cert_path(config).is_some_and(|path| path.exists());
        if registered && !self.re_register && !self.clean {
            return None;
        }
        let device_id = match &self.device_id {
            Some(device_id) => device_id.clone(),
            None => super::command::read_config_string(config, key::DEVICE_ID)?,
        };
        let host = self.c8y_http_host(config).await?;
        Some(c8y_api::registration::device_registration_url(
            &host,
            &device_id,
            Some(password),
        ))
    }
}

async fn create_parent_dir(path: &Utf8Path) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("Failed to create directory {parent}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::command::test_support::touch;
    use super::super::command::test_support::Fixture;
    use super::super::command::OneTimePassword;
    use super::*;
    use crate::cli::common::Cloud;

    #[tokio::test]
    async fn the_pending_registration_url_is_exposed_only_for_generated_passwords() {
        let fx = Fixture::new("[c8y]\nurl = \"example.cumulocity.com\"\n").await;
        let mut command = fx.command(Cloud::c8y(None));
        command.device_id = Some("demo01".into());
        let config = fx.config().await;

        // a password supplied by the user stays secret
        command.one_time_password = OneTimePassword::Supplied("s3cret".into());
        assert_eq!(command.pending_registration_url(&config).await, None);

        command.one_time_password = OneTimePassword::Generated("generated".into());
        let url = command.pending_registration_url(&config).await.unwrap();
        assert!(url.contains("example.cumulocity.com"), "{url}");
        assert!(url.contains("demo01"), "{url}");
        assert!(url.contains("generated"), "{url}");

        // an existing certificate means no registration is pending,
        // unless the run re-registers
        let cert: Utf8PathBuf = config
            .device_cert_path(Some(&command.cloud))
            .unwrap()
            .into();
        touch(&cert);
        assert_eq!(command.pending_registration_url(&config).await, None);
        command.re_register = true;
        assert!(command.pending_registration_url(&config).await.is_some());

        // only the c8y-ca method registers this way
        command.register = RegistrationMethod::Basic;
        assert_eq!(command.pending_registration_url(&config).await, None);
    }
}
