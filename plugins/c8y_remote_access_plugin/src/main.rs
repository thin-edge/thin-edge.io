use async_tungstenite::tokio::ConnectStream;
use auth::Jwt;
use config::TedgeConfig;
use miette::Context;
use miette::IntoDiagnostic;
use proxy::WebsocketSocketProxy;
use rand::prelude::*;
use url::Url;
use ws_stream_tungstenite::WsStream;

mod auth;
mod config;
mod csv;
mod input;
mod proxy;

#[tokio::main]
async fn main() {
    if let Err(e) = fallible_main().await {
        eprintln!("Error: {e:?}");
        std::process::exit(1);
    }
}

async fn fallible_main() -> miette::Result<()> {
    let config = TedgeConfig::read_from_disk().await?;

    let command = input::parse_arguments()?;

    let url = build_proxy_url(&config.c8y.url, command.key())?;
    let jwt = Jwt::retrieve(&config.mqtt).await?;

    let proxy = WebsocketSocketProxy::connect(&url, command.target_address(), jwt)
        .await
        .unwrap();

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
            .unwrap();

        let socket = async_tungstenite::tokio::connect_async(request)
            .await
            .unwrap()
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
