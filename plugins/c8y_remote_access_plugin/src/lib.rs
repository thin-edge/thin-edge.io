use camino::Utf8Path;
use camino::Utf8PathBuf;
use futures::future::try_select;
use futures::future::Either;
use input::parse_arguments;
use miette::miette;
use miette::Context;
use miette::IntoDiagnostic;
use std::io;
use std::process::Stdio;
use tedge_config::log_init;
use tedge_config::tedge_toml::mapper_config::C8yMapperConfig;
use tedge_config::TEdgeConfig;
use tedge_utils::file::change_user_and_group;
use tedge_utils::file::create_directory_with_user_group;
use tedge_utils::file::create_file_with_user_group;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::net::UnixStream;
use url::Url;

use crate::auth::Auth;
pub use crate::input::C8yRemoteAccessPluginOpt;
use crate::input::Command;
use crate::input::RemoteAccessConnect;
use crate::proxy::WebsocketSocketProxy;

mod auth;
mod csv;
mod input;
mod proxy;

const UNIX_SOCKFILE: &str = "/run/c8y-remote-access-plugin.sock";

pub async fn run(opt: C8yRemoteAccessPluginOpt) -> miette::Result<()> {
    let c8y_profile = opt.profile.clone();
    let c8y_profile = c8y_profile.as_deref();

    log_init(
        "c8y_remote_access_plugin",
        &opt.common.log_args,
        &opt.common.config_dir,
    )
    .into_diagnostic()?;

    let tedge_config = TEdgeConfig::load(&opt.common.config_dir)
        .await
        .into_diagnostic()
        .context("Reading tedge config")?;

    let command = parse_arguments(opt)?;

    match command {
        Command::Init(user, group) => declare_supported_operation(
            tedge_config.root_dir(),
            &user,
            &group,
        )
        .await
        .with_context(|| {
            "Failed to initialize c8y-remote-access-plugin. You have to run the command with sudo."
        }),
        Command::Cleanup => {
            remove_supported_operation(tedge_config.root_dir());
            Ok(())
        }
        Command::Connect((command, p)) => {
            let c8y_config = tedge_config.mapper_config(&p).map_err(|e| miette!("{e}"))?;
            proxy(command, &tedge_config, &c8y_config).await
        }
        Command::SpawnChild(command) => {
            spawn_child(command, tedge_config.root_dir(), c8y_profile).await
        }
        Command::TryConnectUnixSocket(command) => match UnixStream::connect(UNIX_SOCKFILE).await {
            Ok(mut unix_stream) => {
                eprintln!("sock: Connected to Unix socket at {UNIX_SOCKFILE}");
                write_request_and_shutdown(&mut unix_stream, c8y_profile, command).await?;
                read_from_stream(&mut unix_stream).await?;
                Ok(())
            }
            Err(_e) => {
                eprintln!("sock: Could not connect to Unix socket at {UNIX_SOCKFILE}. Falling back to spawning a child process");
                spawn_child(command, tedge_config.root_dir(), c8y_profile).await
            }
        },
    }
}

async fn declare_supported_operation(
    config_dir: &Utf8Path,
    user: &str,
    group: &str,
) -> miette::Result<()> {
    let supported_operation_path = supported_operation_path(config_dir);
    create_directory_with_user_group(
        supported_operation_path.parent().unwrap(),
        user,
        group,
        0o755,
    )
    .await
    .into_diagnostic()
    .context("Creating supported operations directory")?;

    if supported_operation_path.exists() {
        change_user_and_group(&supported_operation_path, user, group)
            .await
            .into_diagnostic()
            .context("Changing permissions of supported operations")
    } else {
        create_file_with_user_group(
            supported_operation_path,
            user,
            group,
            0o644,
            Some(
                r#"[exec]
command = "c8y-remote-access-plugin"
topic = "c8y/s/ds"
on_message = "530"
"#,
            ),
        )
        .await
        .into_diagnostic()
        .context("Declaring supported operations")
    }
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

async fn spawn_child(
    command: String,
    config_dir: &Utf8Path,
    c8y_profile: Option<&str>,
) -> miette::Result<()> {
    let exec_path = std::env::args()
        .next()
        .ok_or(miette!("Could not get current process executable"))?;

    let mut command = tokio::process::Command::new(exec_path)
        .args(["--config-dir", config_dir.as_str()])
        .args(
            c8y_profile
                .iter()
                .flat_map(|profile| ["--profile", profile]),
        )
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

#[derive(miette::Diagnostic, Debug, thiserror::Error)]
#[error("Failed while {1}")]
#[diagnostic(help("Check if Unix Socket is readable and writable."))]
struct UnixSocketError<E: std::error::Error + 'static>(#[source] E, &'static str);

async fn write_request_and_shutdown(
    unix_stream: &mut UnixStream,
    profile: Option<&str>,
    command: String,
) -> miette::Result<()> {
    unix_stream
        .writable()
        .await
        .into_diagnostic()
        .context("sock: Socket is not writable")?;

    if let Some(profile) = profile {
        unix_stream
            .write_all(format!("{profile}\n").as_bytes())
            .await
            .into_diagnostic()
            .context("sock: Could not write to socket")?;
    }

    eprintln!("sock: Writing message ({command}) to socket");
    unix_stream
        .write_all(command.as_bytes())
        .await
        .into_diagnostic()
        .context("sock: Could not write to socket")?;
    eprintln!("sock: Message sent");

    unix_stream
        .flush()
        .await
        .into_diagnostic()
        .context("sock: Could not write to socket")?;

    eprintln!("sock: Shutting down writing on the stream, waiting for response...");
    unix_stream
        .shutdown()
        .await
        .into_diagnostic()
        .context("sock: Could not shutdown writing on the stream")?;
    eprintln!("sock: Shut down successful");

    Ok(())
}

async fn read_from_stream(unix_stream: &mut UnixStream) -> miette::Result<()> {
    unix_stream
        .readable()
        .await
        .into_diagnostic()
        .context("sock: Socket is not readable")?;

    eprintln!("sock: Reading response...");
    let stream = BufReader::new(unix_stream);
    let mut lines = stream.lines();

    while let Ok(maybe_line) = lines.next_line().await {
        match maybe_line {
            Some(line) => {
                eprintln!("{line}");
                match line.as_str() {
                    str if str == SUCCESS_MESSAGE => {
                        eprintln!("sock: Detected successful response");
                        return Ok(());
                    }
                    "STOPPING" => {
                        eprintln!("sock: Detected error response");
                        break;
                    }
                    _ => continue,
                }
            }
            None => {
                eprintln!("sock: Connection closed by peer");
                break;
            }
        }
    }

    Err(UnixSocketError(
        io::Error::other(format!(
            "sock: Did not receive expected response from socket. Expected = '{SUCCESS_MESSAGE}'"
        )),
        "checking the response from the unix socket",
    )
    .into())
}

async fn proxy(
    command: RemoteAccessConnect,
    config: &TEdgeConfig,
    c8y_config: &C8yMapperConfig,
) -> miette::Result<()> {
    let host = c8y_config
        .cloud_specific
        .http
        .or_config_not_set()
        .into_diagnostic()?
        .to_string();
    let url = build_proxy_url(host.as_str(), command.key())?;
    let auth = Auth::retrieve(config, c8y_config)
        .await.context("Failed when requesting JWT from Cumulocity or invalid username/password credentials are given")?;
    let client_config = config.cloud_client_tls_config();

    let proxy = WebsocketSocketProxy::connect(
        &url,
        command.target_address(),
        auth,
        Some(client_config),
        &config.proxy,
    )
    .await?;

    proxy.run().await;
    Ok(())
}

fn supported_operation_path(config_dir: &Utf8Path) -> Utf8PathBuf {
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
