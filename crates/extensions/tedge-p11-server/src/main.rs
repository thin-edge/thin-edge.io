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
use camino::Utf8Path;
use camino::Utf8PathBuf;
use clap::crate_version;
use clap::Parser;
use flockfile::Flockfile;
use flockfile::FlockfileError;
use serde::Deserialize;
use tedge_p11::CryptokiConfigDirect;
use tedge_p11::TedgeP11Client;
use tedge_p11::TedgeP11Server;
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

    // Treat an empty env var / flag value the same as unset, so e.g. an empty
    // TEDGE_DEVICE_CRYPTOKI_PIN falls back to tedge.toml instead of overriding it with "".
    // (clap parses a set-but-empty env var as `Some("")`.) This matters for service managers
    // that pass the variables unconditionally.
    let (pin, Some(module_path), socket_path, uri) = (
        args.pin
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| toml_config.pin()),
        args.module_path
            .filter(|p| !p.as_str().is_empty())
            .or_else(|| toml_config.module_path()),
        args.socket_path
            .filter(|p| !p.as_str().is_empty())
            .unwrap_or_else(|| toml_config.socket_path()),
        args.uri
            .filter(|s| !s.is_empty())
            .or_else(|| toml_config.uri()),
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

    info!("Starting tedge-p11-server {}", crate_version!());

    let config = try_read_config(args).await?;
    let cryptoki_config = CryptokiConfigDirect {
        module_path: config.module_path,
        pin: tedge_p11::SecretString::new(config.pin),
        uri: config.uri.filter(|s| !s.is_empty()).map(|s| {
            let v = s.into_boxed_str();
            Arc::<str>::from(v)
        }),
    };
    let socket_path = config.socket_path;

    info!(?cryptoki_config, "Using cryptoki configuration");

    // make sure that if we bind to unix socket in the program, it's removed on exit, and that the
    // socket lock (if any) is held for the whole lifetime of the server
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
            let (listener, lock) = bind_socket(&socket_path)?;
            (
                listener,
                Some((OwnedSocketFileDropGuard(socket_path), lock)),
            )
        }
    };
    info!(listener = ?listener.local_addr().as_ref().ok().and_then(|s| s.as_pathname()), "Server listening");
    listener.set_nonblocking(true)?;
    let listener = tokio::net::UnixListener::from_std(listener)?;
    let service = tedge_p11::pkcs11::Cryptoki::new(cryptoki_config)
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

/// Binds a UNIX socket listener at `socket_path`, reclaiming a stale socket file if necessary.
///
/// `UnixListener::bind` fails with `AddrInUse` if a file already exists at the path, even when no
/// process is listening on it — which happens when a previous run was killed before its socket file
/// could be cleaned up (e.g. the process was terminated abruptly). In that case we want to remove
/// the stale file and bind again, but only when it is safe to do so — never stealing the socket
/// from a healthy instance.
///
/// To make this race-free we guard socket creation with an `flock`-based lock file
/// (`<socket_path>.lock`), which a running server holds for its whole lifetime and the kernel
/// releases automatically when the process dies, even on `SIGKILL` (see
/// <https://gavv.net/articles/unix-socket-reuse/>). Acquiring the lock therefore proves that no
/// other server is running, so any leftover socket file is definitively stale and can be removed
/// before binding. If the lock is already held, another instance owns the socket and we bail
/// without touching it.
///
/// The lock lives next to the socket so that, when the socket is shared into containers via a
/// volume, all instances that could contend for it (necessarily on the same host kernel, since a
/// UNIX socket is host-local) see the same lock. On the rare filesystem where `flock` silently
/// no-ops we could still hold the lock while another server is actually listening, so before
/// deleting a leftover socket we ping it as a final safety check and refuse to touch it if a server
/// answers.
///
/// The returned [`Flockfile`] must be kept alive for as long as the server runs.
fn bind_socket(socket_path: &Utf8Path) -> anyhow::Result<(UnixListener, Flockfile)> {
    let lock_path = format!("{socket_path}.lock");
    let lock = match Flockfile::new_lock(&lock_path) {
        Ok(lock) => lock,
        // `FromNix` is the non-blocking `flock()` call reporting the lock is already held: another
        // instance owns the socket.
        Err(FlockfileError::FromNix { .. }) => {
            anyhow::bail!("Another instance is already listening on the socket at '{socket_path}'");
        }
        // `FromIo` is a genuine problem creating or opening the lock file itself.
        Err(err) => {
            return Err(err)
                .with_context(|| format!("Failed to acquire socket lock at '{lock_path}'"));
        }
    };

    match UnixListener::bind(socket_path) {
        Ok(listener) => Ok((listener, lock)),
        Err(err) if err.kind() == std::io::ErrorKind::AddrInUse => {
            // We hold the lock, so no other server should be running and the socket file should be
            // stale. Guard against `flock` having silently no-op'd on this filesystem by pinging
            // the socket first: if a server actually answers, refuse to steal it. `with_ready_check`
            // already pings once (discarding the result); we ping again to observe the answer.
            let is_alive = TedgeP11Client::with_ready_check(Arc::from(socket_path.as_std_path()))
                .ping()
                .is_ok();
            if is_alive {
                anyhow::bail!(
                    "Another instance is already listening on the socket at '{socket_path}'"
                );
            }
            warn!(%socket_path, "Removing stale socket file left over by a previous run");
            std::fs::remove_file(socket_path).with_context(|| {
                format!("Failed to remove stale socket file at '{socket_path}'")
            })?;
            let listener = UnixListener::bind(socket_path)
                .with_context(|| format!("Failed to bind to socket at '{socket_path}'"))?;
            Ok((listener, lock))
        }
        Err(err) => {
            Err(err).with_context(|| format!("Failed to bind to socket at '{socket_path}'"))
        }
    }
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

    #[tokio::test]
    async fn empty_env_values_fall_back_to_tedge_toml() {
        let dir = tempfile::tempdir().unwrap();
        let config_dir = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        std::fs::write(
            config_dir.join("tedge.toml"),
            "[device.cryptoki]\nmodule_path = \"/lib/module.so\"\npin = \"123456\"\n",
        )
        .unwrap();

        // An empty env var / flag value (as a service manager passes for an unset variable)
        // must not override the configured value.
        let args = Args {
            socket_path: None,
            module_path: None,
            pin: Some(String::new()),
            uri: Some(String::new()),
            log_level: None,
            config_dir,
        };

        let config = try_read_config(args).await.unwrap();
        assert_eq!(config.pin, "123456"); // empty pin fell back to tedge.toml
        assert_eq!(config.module_path.as_str(), "/lib/module.so");
        assert_eq!(config.uri, None); // empty uri treated as unset
    }

    #[test]
    fn bind_socket_reclaims_a_stale_socket_file() {
        let dir = tempfile::tempdir().unwrap();
        let socket_path = Utf8PathBuf::from_path_buf(dir.path().join("test.sock")).unwrap();

        // Simulate an unclean shutdown: a socket file exists but nothing holds the lock and
        // nothing is listening on it. std's UnixListener does not unlink the file on drop, so the
        // path lingers.
        drop(UnixListener::bind(&socket_path).unwrap());
        assert!(socket_path.exists());

        // A plain bind would fail with AddrInUse; bind_socket should reclaim the stale file.
        let (listener, _lock) =
            bind_socket(&socket_path).expect("should reclaim stale socket file");
        drop(listener);
    }

    #[test]
    fn bind_socket_refuses_when_another_instance_holds_the_lock() {
        let dir = tempfile::tempdir().unwrap();
        let socket_path = Utf8PathBuf::from_path_buf(dir.path().join("test.sock")).unwrap();

        // A running instance holds the socket lock (and the listener) for its whole lifetime.
        let (_listener, _lock) = bind_socket(&socket_path).unwrap();

        // A second start must refuse, because the lock is held, and must leave the live socket in
        // place.
        let err = bind_socket(&socket_path).unwrap_err();
        assert!(
            err.to_string().contains("Another instance"),
            "unexpected error: {err:#}"
        );
        assert!(socket_path.exists());
    }

    /// A no-op service used only to stand up a real server for the "refuse" test below. A ping is
    /// answered by the server itself, so none of these methods are ever called.
    struct StubService;

    impl tedge_p11::service::TedgeP11Service for StubService {
        fn choose_scheme(
            &self,
            _: tedge_p11::service::ChooseSchemeRequest,
        ) -> anyhow::Result<tedge_p11::service::ChooseSchemeResponse> {
            unimplemented!()
        }
        fn sign(
            &self,
            _: tedge_p11::service::SignRequestWithSigScheme,
        ) -> anyhow::Result<tedge_p11::service::SignResponse> {
            unimplemented!()
        }
        fn get_public_key_pem(&self, _: Option<&str>) -> anyhow::Result<String> {
            unimplemented!()
        }
        fn get_tokens_uris(&self) -> anyhow::Result<Vec<String>> {
            unimplemented!()
        }
        fn list_tokens(&self) -> anyhow::Result<tedge_p11::service::ListTokensResponse> {
            unimplemented!()
        }
        fn change_pin(
            &self,
            _: tedge_p11::service::ChangePinRequest,
        ) -> anyhow::Result<tedge_p11::service::ChangePinResponse> {
            unimplemented!()
        }
        fn delete_key(
            &self,
            _: tedge_p11::service::DeleteKeyRequest,
        ) -> anyhow::Result<tedge_p11::service::DeleteKeyResponse> {
            unimplemented!()
        }
        fn create_key(
            &self,
            _: tedge_p11::service::CreateKeyRequest,
        ) -> anyhow::Result<tedge_p11::service::CreateKeyResponse> {
            unimplemented!()
        }
        fn init_token(
            &self,
            _: tedge_p11::service::InitTokenRequest,
        ) -> anyhow::Result<tedge_p11::service::InitTokenResponse> {
            unimplemented!()
        }
    }

    /// Belt-and-suspenders for the hybrid approach: even if `flock` silently no-ops (so the lock is
    /// acquired despite a live server), the ping guard must stop us from stealing a socket that a
    /// server is actually answering on. Here the live server is stood up directly on the socket
    /// without going through the lock, mimicking that degraded-`flock` case.
    #[tokio::test]
    async fn bind_socket_refuses_when_a_server_is_listening() {
        let dir = tempfile::tempdir().unwrap();
        let socket_path = Utf8PathBuf::from_path_buf(dir.path().join("test.sock")).unwrap();

        // Run a real tedge-p11-server on the socket so it answers pings.
        let listener = tokio::net::UnixListener::bind(&socket_path).unwrap();
        let server = TedgeP11Server::new(StubService).unwrap();
        tokio::spawn(async move { server.serve(listener).await });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // bind_socket must refuse to touch a live server's socket, and leave it in place.
        let path = socket_path.clone();
        let err = tokio::task::spawn_blocking(move || bind_socket(&path).unwrap_err())
            .await
            .unwrap();
        assert!(
            err.to_string().contains("Another instance"),
            "unexpected error: {err:#}"
        );
        assert!(socket_path.exists());
    }
}
