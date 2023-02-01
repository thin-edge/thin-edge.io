use std::path::Path;

use async_tungstenite::tokio::ConnectStream;
use config::supported_operation_path;
use config::C8yUrl;
use miette::Context;
use miette::IntoDiagnostic;
use rand::prelude::*;
use tedge_config::ConfigRepository;
use tedge_config::TEdgeConfig;
use tedge_config::TEdgeConfigLocation;
use tedge_config::TEdgeConfigRepository;
use tedge_utils::file::create_file_with_user_group;
use url::Url;
use ws_stream_tungstenite::WsStream;

use crate::auth::Jwt;
use crate::input::Command;
use crate::input::RemoteAccessConnect;
use crate::proxy::WebsocketSocketProxy;

mod auth;
mod config;
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
    let C8yUrl(host) = C8yUrl::retrieve(&config)?;
    let url = build_proxy_url(&host, command.key())?;
    let jwt = Jwt::retrieve(&config)
        .await
        .context("Failed when requesting JWT from Cumulocity")?;

    let proxy = WebsocketSocketProxy::connect(&url, command.target_address(), jwt).await?;

    proxy.run().await;
    Ok(())
}

fn build_proxy_url(cumulocity_host: &str, key: &str) -> miette::Result<Url> {
    format!("wss://{cumulocity_host}/service/remoteaccess/device/{key}")
        .parse()
        .into_diagnostic()
        .context("Creating websocket URL")
}

struct Websocket {
    socket: WsStream<ConnectStream>,
}

fn generate_sec_websocket_key() -> String {
    let mut rng = rand::thread_rng();
    let mut bytes = [0u8; 16];
    rng.fill_bytes(&mut bytes);
    base64::encode(bytes)
}

impl Websocket {
    async fn new(url: &Url, authorization: String) -> miette::Result<Self> {
        let request = http::Request::builder()
            .header("Authorization", authorization)
            .header("Sec-WebSocket-Key", generate_sec_websocket_key())
            .header("Host", url.host_str().unwrap())
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("sec-websocket-version", "13")
            .uri(url.to_string())
            .body(())
            .into_diagnostic()
            .context("Instantiating Websocket connection")?;

        let socket = async_tungstenite::tokio::connect_async(request)
            .await
            .into_diagnostic()
            .context("Connecting to Websocket")?
            .0;
        Ok(Websocket {
            socket: WsStream::new(socket),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_key_is_base64_encoded_16_byte_sequence() {
        let key = generate_sec_websocket_key();

        let decoded = base64::decode(key).unwrap();

        assert_eq!(decoded.len(), 16);
    }

    #[test]
    fn generated_key_is_ascii() {
        let key = generate_sec_websocket_key();

        assert!(key.is_ascii());
    }

    #[test]
    fn generated_keys_are_unique_per_connection() {
        let key_1 = generate_sec_websocket_key();
        let key_2 = generate_sec_websocket_key();

        assert_ne!(key_1, key_2);
    }
}
