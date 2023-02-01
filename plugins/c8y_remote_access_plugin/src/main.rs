use std::path::Path;
use std::path::PathBuf;

use miette::Context;
use miette::IntoDiagnostic;
use tedge_config::C8yUrlSetting;
use tedge_config::ConfigRepository;
use tedge_config::ConfigSettingAccessor;
use tedge_config::TEdgeConfig;
use tedge_config::TEdgeConfigLocation;
use tedge_config::TEdgeConfigRepository;
use tedge_utils::file::create_file_with_user_group;
use url::Url;

use crate::auth::Jwt;
use crate::input::Command;
use crate::input::RemoteAccessConnect;
use crate::proxy::WebsocketSocketProxy;

mod auth;
mod csv;
mod input;
mod proxy;

#[tokio::main]
async fn main() -> miette::Result<()> {
    let config_dir = TEdgeConfigLocation::default();
    let tedge_config = TEdgeConfigRepository::new(config_dir.clone())
        .load()
        .into_diagnostic()
        .context("Reading tedge config")?;

    let command = input::parse_arguments()?;

    match command {
        Command::Init => declare_supported_operation(config_dir.tedge_config_root_path()),
        Command::Cleanup => remove_supported_operation(config_dir.tedge_config_root_path()),
        Command::Connect(command) => proxy(command, tedge_config).await,
    }
}

fn declare_supported_operation(config_dir: &Path) -> miette::Result<()> {
    create_file_with_user_group(
        supported_operation_path(config_dir),
        "tedge",
        "tedge",
        0o644,
        Some(
            r#"[exec]
command = "/usr/bin/c8y-remote-access-plugin"
topic = "c8y/s/ds"
on_message = "530"
"#,
        ),
    )
    .into_diagnostic()
    .context("Declaring supported operations")
}

fn remove_supported_operation(config_dir: &Path) -> miette::Result<()> {
    let path = supported_operation_path(config_dir);
    std::fs::remove_file(&path)
        .into_diagnostic()
        .with_context(|| format!("Removing supported operation at {}", path.display()))
}

async fn proxy(command: RemoteAccessConnect, config: TEdgeConfig) -> miette::Result<()> {
    let host = config.query(C8yUrlSetting).into_diagnostic()?;
    let url = build_proxy_url(host.as_str(), command.key())?;
    let jwt = Jwt::retrieve(&config)
        .await
        .context("Failed when requesting JWT from Cumulocity")?;

    let proxy = WebsocketSocketProxy::connect(&url, command.target_address(), jwt).await?;

    proxy.run().await;
    Ok(())
}

fn supported_operation_path(config_dir: &Path) -> PathBuf {
    let mut path = config_dir.to_owned();
    path.push("operations/c8y/c8y_RemoteAccessConnect");
    path
}

fn build_proxy_url(cumulocity_host: &str, key: &str) -> miette::Result<Url> {
    format!("wss://{cumulocity_host}/service/remoteaccess/device/{key}")
        .parse()
        .into_diagnostic()
        .context("Creating websocket URL")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_supported_operation_path() {
        assert_eq!(
            supported_operation_path("/etc/tedge".as_ref()),
            PathBuf::from("/etc/tedge/operations/c8y/c8y_RemoteAccessConnect")
        );
    }
}
