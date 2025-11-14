//! thin-edge.io PKCS#11 server.
//!
//! The purpose of this crate is to allow thin-edge services that possibly run in containers to access PKCS#11 tokens in
//! all of our supported architectures.
//!
//! There are 2 main problems with using a PKCS#11 module directly by thin-edge:
//! 1. One needs to use a dynamic loader to load the PKCS#11 module, which is not possible in statically compiled musl
//! 2. When thin-edge runs in a container, additional setup needs to be done by the user to expose cryptographic tokens
//!    in the container, using software like p11-kit.
//!
//! To avoid extra dependencies and possibly implement new features in the future, it was decided that thin-edge.io will
//! provide its own bundled p11-kit-like service.

use std::os::unix::net::UnixListener;
use std::sync::Arc;

use anyhow::Context;
use camino::Utf8PathBuf;
use clap::command;
use clap::Parser;
use serde::Deserialize;
use tedge_p11_server::CryptokiConfigDirect;
use tedge_p11_server::TedgeP11Server;
use tokio::signal::unix::SignalKind;
use tracing::debug;
use tracing::error;
use tracing::info;
use tracing::warn;
use tracing::Level;
use tracing_subscriber::EnvFilter;

/// thin-edge.io service for passing PKCS#11 cryptographic tokens.
#[derive(Debug, Clone, PartialEq, Eq, Parser)]
#[command(version)]
pub struct Args {
    /// A path where the UNIX socket listener will be created.
    #[arg(
        long,
        env = "TEDGE_DEVICE_CRYPTOKI_SOCKET_PATH",
        hide_env_values = true
    )]
    socket_path: Option<Utf8PathBuf>,

    /// The path to the PKCS#11 module.
    #[arg(
        long,
        env = "TEDGE_DEVICE_CRYPTOKI_MODULE_PATH",
        hide_env_values = true
    )]
    module_path: Option<Utf8PathBuf>,

    /// The PIN for the PKCS#11 token.
    #[arg(long, env = "TEDGE_DEVICE_CRYPTOKI_PIN", hide_env_values = true)]
    pin: Option<String>,

    /// A URI of the token/object to use.
    ///
    /// See RFC #7512.
    #[arg(long, env = "TEDGE_DEVICE_CRYPTOKI_URI", hide_env_values = true)]
    uri: Option<String>,

    /// Configures the logging level.
    ///
    /// One of error/warn/info/debug/trace. Logs with verbosity lower or equal to the selected level
    /// will be printed, i.e. warn prints ERROR and WARN logs and trace prints logs of all levels.
    #[arg(long)]
    log_level: Option<Level>,

    /// [env: TEDGE_CONFIG_DIR, default: /etc/tedge]
    #[arg(
        long,
        env = "TEDGE_CONFIG_DIR",
        default_value = "/etc/tedge",
        hide_env_values = true,
        hide_default_value = true,
        hide_env = true,
        global = true
    )]
    config_dir: Utf8PathBuf,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct PartialTedgeToml {
    #[serde(default)]
    device: PartialDeviceConfig,
}

#[derive(Debug, Default, Deserialize, PartialEq, Eq)]
struct PartialDeviceConfig {
    #[serde(default)]
    cryptoki: TomlCryptokiConfig,
}

#[derive(Debug, Default, Deserialize, PartialEq, Eq)]
struct TomlCryptokiConfig {
    pin: Option<String>,
    module_path: Option<Utf8PathBuf>,
    socket_path: Option<Utf8PathBuf>,
    uri: Option<String>,
}

fn default_socket_path() -> Utf8PathBuf {
    "./tedge-p11-server.sock".into()
}

fn default_pin() -> String {
    "123456".into()
}

/// Cache the result of reading a Cryptoki configuration from tedge.toml
///
/// The point is to warn the user if this file could not be read (either missing or incomplete),
/// but only when actually used, i.e. when the user didn't provide the required config
/// neither using env variables nor cli flags.
///
/// This struct is used to defer error reporting,
/// - making sure the user is warned but only when the file could have been used
/// - and only once, even if several setting are missing.
struct TomlConfig {
    read_result: Result<TomlCryptokiConfig, Option<String>>,
    user_warned: bool,
}

impl TomlConfig {
    async fn read_tedge_toml(toml_path: &Utf8PathBuf) -> Self {
        TomlConfig {
            read_result: try_read_tedge_toml(toml_path).await,
            user_warned: false,
        }
    }

    fn config(&mut self) -> Option<&TomlCryptokiConfig> {
        match self.read_result.as_ref() {
            Ok(config) => Some(config),
            Err(None) => None, // don't log anything if tedge.toml doesn't exist
            Err(Some(err)) => {
                if !self.user_warned {
                    warn!("{err}");
                    self.user_warned = true;
                }
                None
            }
        }
    }

    fn pin(&mut self) -> String {
        self.config()
            .and_then(|config| config.pin.to_owned())
            .unwrap_or_else(|| {
                warn!("missing pin => use default value");
                default_pin()
            })
    }

    fn module_path(&mut self) -> Option<Utf8PathBuf> {
        self.config()
            .and_then(|config| config.module_path.to_owned())
            .or_else(|| {
                error!("missing required module-path => abort");
                None
            })
    }

    fn socket_path(&mut self) -> Utf8PathBuf {
        self.config()
            .and_then(|config| config.socket_path.to_owned())
            .unwrap_or_else(|| {
                warn!("missing socket-path => use default value");
                default_socket_path()
            })
    }

    fn uri(&mut self) -> Option<String> {
        self.config().and_then(|config| config.uri.to_owned())
    }
}

struct ValidConfig {
    pin: String,
    module_path: Utf8PathBuf,
    socket_path: Utf8PathBuf,
    uri: Option<String>,
}
async fn try_read_tedge_toml(
    toml_path: &Utf8PathBuf,
) -> Result<TomlCryptokiConfig, Option<String>> {
    let toml = tokio::fs::read_to_string(&toml_path).await.map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            None
        } else {
            Some(format!("Failed to read {toml_path}: {err}"))
        }
    })?;
    let config: PartialTedgeToml = toml::from_str(&toml)
        .map_err(|_| Some(format!("Failed to parse {toml_path}: invalid TOML")))?;
    Ok(config.device.cryptoki)
}

async fn try_read_config(args: Args) -> anyhow::Result<ValidConfig> {
    let toml_path = args.config_dir.join("tedge.toml");
    let mut toml_config = TomlConfig::read_tedge_toml(&toml_path).await;

    let (pin, Some(module_path), socket_path, uri) = (
        args.pin.unwrap_or_else(|| toml_config.pin()),
        args.module_path.or_else(|| toml_config.module_path()),
        args.socket_path
            .unwrap_or_else(|| toml_config.socket_path()),
        args.uri.or_else(|| toml_config.uri()),
    ) else {
        anyhow::bail!("Missing configuration values. Please set them in `tedge.toml` or pass them as command-line arguments.")
    };

    Ok(ValidConfig {
        pin,
        module_path,
        socket_path,
        uri,
    })
}

// Control when to use console colors (`stdout` and `stderr` is a TTY, `NO_COLOR` is not set)
static USE_COLOR: yansi::Condition = yansi::Condition::from(|| {
    yansi::Condition::stdouterr_are_tty() && yansi::Condition::no_color()
});

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let if_dev_logging = std::env::var("RUST_LOG").is_ok();

    tracing_subscriber::fmt()
        .with_file(if_dev_logging)
        .with_line_number(if_dev_logging)
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(args.log_level.unwrap_or(Level::INFO).into())
                .from_env()
                .unwrap(),
        )
        .init();

    yansi::whenever(USE_COLOR);

    let args = Args::parse();
    let config = try_read_config(args).await?;
    let cryptoki_config = CryptokiConfigDirect {
        module_path: config.module_path,
        pin: tedge_p11_server::SecretString::new(config.pin),
        uri: config.uri.filter(|s| !s.is_empty()).map(|s| {
            let v = s.into_boxed_str();
            Arc::<str>::from(v)
        }),
    };
    let socket_path = config.socket_path;

    info!(?cryptoki_config, "Using cryptoki configuration");

    // make sure that if we bind to unix socket in the program, it's removed on exit
    let (listener, _drop_guard) = {
        let mut systemd_listeners = sd_listen_fds::get()
            .context("Failed to obtain activated sockets from systemd")?
            .into_iter();
        if systemd_listeners.len() > 1 {
            warn!("Received multiple sockets but only first is used, rest are ignored");
        }
        if let Some((name, fd)) = systemd_listeners.next() {
            debug!(?name, "Using socket passed by systemd");
            let listener = UnixListener::from(fd);
            (listener, None)
        } else {
            debug!("No sockets from systemd, creating a standalone socket");
            let socket_dir = socket_path.parent();
            if let Some(socket_dir) = socket_dir {
                if !socket_dir.exists() {
                    tokio::fs::create_dir_all(socket_dir)
                        .await
                        .context(format!(
                            "error creating parent directories for {socket_dir:?}"
                        ))?;
                }
            }
            let listener = UnixListener::bind(&socket_path)
                .with_context(|| format!("Failed to bind to socket at '{socket_path}'"))?;
            (listener, Some(OwnedSocketFileDropGuard(socket_path)))
        }
    };
    info!(listener = ?listener.local_addr().as_ref().ok().and_then(|s| s.as_pathname()), "Server listening");
    listener.set_nonblocking(true)?;
    let listener = tokio::net::UnixListener::from_std(listener)?;
    let service = tedge_p11_server::pkcs11::Cryptoki::new(cryptoki_config)
        .context("Failed to create the signing service")?;
    let server = TedgeP11Server::new(service)?;
    tokio::spawn(async move { server.serve(listener).await });

    // by capturing SIGINT and SIGERM, we allow owned socket drop guard to run before exit
    let mut sigint = tokio::signal::unix::signal(SignalKind::interrupt()).unwrap();
    let mut sigterm = tokio::signal::unix::signal(SignalKind::terminate()).unwrap();

    tokio::select! {
        _ = sigint.recv() => {}
        _ = sigterm.recv() => {}
    }

    Ok(())
}

struct OwnedSocketFileDropGuard(Utf8PathBuf);

// necessary for correct unix socket deletion when server exits with an error
impl Drop for OwnedSocketFileDropGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_example_config() {
        let tedge_toml = r#"[device.cryptoki]
mode = "socket"
pin = "123456"
socket_path = "/var/run/tedge-p11-server/tedge-p11-server.sock"
module_path = """#;

        let config: PartialTedgeToml = toml::from_str(tedge_toml).unwrap();

        assert_eq!(
            config,
            PartialTedgeToml {
                device: PartialDeviceConfig {
                    cryptoki: TomlCryptokiConfig {
                        module_path: Some("".into()),
                        pin: Some("123456".to_owned()),
                        socket_path: Some("/var/run/tedge-p11-server/tedge-p11-server.sock".into()),
                        uri: None,
                    }
                }
            }
        )
    }
}
