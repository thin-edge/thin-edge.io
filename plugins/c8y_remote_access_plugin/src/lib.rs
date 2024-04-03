use std::fmt::Display;
use std::fmt::Formatter;
use std::path::PathBuf;
use std::process::Stdio;

use camino::Utf8Path;
use camino::Utf8PathBuf;
use futures::future::try_select;
use futures::future::Either;
use input::parse_arguments;
use miette::miette;
use miette::Context;
use miette::IntoDiagnostic;
use tedge_config::TEdgeConfig;
use tedge_utils::file::create_directory_with_user_group;
use tedge_utils::file::create_file_with_user_group;
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
use toml::Table;
use url::Url;

pub use crate::input::C8yRemoteAccessPluginOpt;
use crate::input::Command;
use crate::input::RemoteAccessConnect;
use crate::proxy::WebsocketSocketProxy;

mod csv;
mod input;
mod proxy;

pub async fn run(opt: C8yRemoteAccessPluginOpt) -> miette::Result<()> {
    let config_dir = opt.get_config_location();

    let tedge_config = TEdgeConfig::try_new(config_dir.clone())
        .into_diagnostic()
        .context("Reading tedge config")?;

    let command = parse_arguments(opt)?;

    match command {
        Command::Init => declare_supported_operation(config_dir.tedge_config_root_path())
            .with_context(|| {
                "Failed to initialize c8y-remote-access-plugin. You have to run the command with sudo."
            }),
        Command::Cleanup => {
            remove_supported_operation(config_dir.tedge_config_root_path());
            Ok(())
        }
        Command::Connect(command) => proxy(command, tedge_config).await,
        Command::SpawnChild(command) => spawn_child(command, config_dir.tedge_config_root_path()).await,
    }
}

fn declare_supported_operation(config_dir: &Utf8Path) -> miette::Result<()> {
    let supported_operation_path = supported_operation_path(config_dir);
    create_directory_with_user_group(
        supported_operation_path.parent().unwrap(),
        "tedge",
        "tedge",
        0o755,
    )
    .into_diagnostic()
    .context("Creating supported operations directory")?;

    create_file_with_user_group(
        supported_operation_path,
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

fn remove_supported_operation(config_dir: &Utf8Path) {
    let path = supported_operation_path(config_dir);
    // Ignore the error as the file may already have been deleted (#1869)
    let _ = std::fs::remove_file(path);
}

static SUCCESS_MESSAGE: &str = "CONNECTED";

#[derive(miette::Diagnostic, Debug, thiserror::Error)]
#[error("Failed while {1}")]
#[diagnostic(help(
    "This should never happen. It's very likely a bug in the c8y remote access plugin."
))]
struct Unreachable<E: std::error::Error + 'static>(#[source] E, &'static str);

async fn spawn_child(command: String, config_dir: &Utf8Path) -> miette::Result<()> {
    let exec_path = get_executable_path(config_dir).await?;

    let mut command = tokio::process::Command::new(exec_path)
        .arg("--child")
        .arg(command)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .into_diagnostic()
        .context("Failed to spawn child process")?;

    let mut stdout = BufReader::new(command.stdout.take().unwrap());
    let mut stderr = BufReader::new(command.stderr.take().unwrap());

    let copy_error_messages = tokio::task::spawn(async move {
        let mut line = String::new();
        while let Ok(amount) = stderr.read_line(&mut line).await {
            if amount == 0 {
                break;
            }
            eprint!("{line}");
            line.clear();
        }
    });

    let wait_for_connection = tokio::task::spawn(async move {
        let mut line = String::new();
        while stdout.read_line(&mut line).await.is_ok() {
            // Copy the output to the parent process stdout to ensure anything we might
            // print doesn't get lost before we connect
            print!("{line}");
            if line.trim() == SUCCESS_MESSAGE {
                break;
            }
            line.clear();
        }
    });

    let wait_for_failure = tokio::task::spawn(async move { command.wait().await });

    match try_select(wait_for_connection, wait_for_failure).await {
        Ok(Either::Left(_)) => Ok(()),
        Ok(Either::Right((Ok(code), _))) => {
            copy_error_messages
                .await
                .map_err(|e| Unreachable(e, "copying stderr from child process"))?;
            let code = code.code().unwrap_or(1);
            std::process::exit(code)
        }
        Ok(Either::Right((Err(e), _))) => Err(e)
            .into_diagnostic()
            .context("Failed to retrieve exit code from child process"),
        Err(Either::Left((e, _))) => {
            Err(Unreachable(e, "waiting for the connection to be established").into())
        }
        Err(Either::Right((e, _))) => Err(Unreachable(e, "waiting for the process to exit").into()),
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum WsProtocol {
    Ws,
    Wss,
}

impl Display for WsProtocol {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ws => "ws".fmt(f),
            Self::Wss => "wss".fmt(f),
        }
    }
}

async fn proxy(command: RemoteAccessConnect, config: TEdgeConfig) -> miette::Result<()> {
    let host = &config.c8y.proxy.client.host;
    let port = config.c8y.proxy.client.port;
    let protocol = config
        .c8y
        .proxy
        .cert_path
        .or_none()
        .map_or(WsProtocol::Ws, |_| WsProtocol::Wss);
    let url = build_proxy_url(protocol, host, port, command.key())?;
    let client_config = config
        .http
        .client_tls_config()
        .map_err(|e| miette!("{e}"))?;

    let proxy =
        WebsocketSocketProxy::connect(&url, command.target_address(), client_config).await?;

    proxy.run().await;
    Ok(())
}

fn supported_operation_path(config_dir: &Utf8Path) -> Utf8PathBuf {
    let mut path = config_dir.to_owned();
    path.push("operations/c8y/c8y_RemoteAccessConnect");
    path
}

fn build_proxy_url(
    protocol: WsProtocol,
    auth_proxy_host: &str,
    auth_proxy_port: u16,
    key: &str,
) -> miette::Result<Url> {
    format!(
        "{protocol}://{auth_proxy_host}:{auth_proxy_port}/c8y/service/remoteaccess/device/{key}"
    )
    .parse()
    .into_diagnostic()
    .context("Creating websocket URL")
}

async fn get_executable_path(config_dir: &Utf8Path) -> miette::Result<PathBuf> {
    let operation_path = supported_operation_path(config_dir);

    let content = tokio::fs::read_to_string(&operation_path)
        .await
        .into_diagnostic()
        .with_context(|| {
            format!("The operation file {operation_path} does not exist or is not readable.")
        })?;

    let operation: Table = content
        .parse()
        .into_diagnostic()
        .with_context(|| format!("Failed to parse {operation_path} file"))?;

    Ok(PathBuf::from(
        operation["exec"]["command"]
            .as_str()
            .ok_or_else(|| miette!("Failed to read command from {operation_path} file"))?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_supported_operation_path() {
        assert_eq!(
            supported_operation_path("/etc/tedge".as_ref()),
            Utf8PathBuf::from("/etc/tedge/operations/c8y/c8y_RemoteAccessConnect")
        );
    }

    #[test]
    fn cleanup_existing_operation() {
        let dir = tempfile::tempdir().unwrap();

        let operation_path = create_example_operation(dir.path().try_into().unwrap());
        remove_supported_operation(dir.path().try_into().unwrap());

        assert!(!operation_path.exists());
    }

    #[test]
    fn cleanup_non_existent_operation() {
        // Verify that this doesn't panic
        remove_supported_operation(
            "/tmp/already-deleted-operations-via-removal-of-tedge-agent".into(),
        );
    }

    fn create_example_operation(dir: &Utf8Path) -> Utf8PathBuf {
        let operation_path = supported_operation_path(dir);
        std::fs::create_dir_all(operation_path.parent().unwrap()).unwrap();
        std::fs::File::create(&operation_path).unwrap();
        assert!(operation_path.exists());
        operation_path
    }
}
