use std::borrow::Cow;
use std::fmt;
use std::io::IsTerminal;
use std::io::Write;
use std::sync::mpsc;
use std::time::Duration;
use std::time::Instant;

use crate::system_services::SystemServiceError;
use crate::system_services::SystemServiceManager;
use camino::Utf8Path;
use tedge_config::models::auth_method::AuthMethod;
use tedge_config::models::auth_method::AuthType;
use tedge_config::tedge_toml::MultiError;
use yansi::Paint as _;

use crate::bridge::BridgeConfig;
use crate::bridge::BridgeLocation;
use crate::error::TEdgeError;

use super::common::MaybeBorrowedCloud;
use super::disconnect::error::DisconnectBridgeError;
use super::CertError;
use super::ConnectError;

#[macro_export]
macro_rules! warning {
    ($($arg:tt)+) => ({
        use yansi::Paint as _; eprintln!("{} {}", "warning:".yellow().bold(), format_args!($($arg)+))
    });
}

#[macro_export]
macro_rules! error {
    ($($arg:tt)+) => ({
        use yansi::Paint as _; eprintln!("{} {}", "error:".red().bold(), format_args!($($arg)+))
    });
}

/// A terminal progress indicator that prints a loading spinner with a message
///
/// Upon calling [Spinner::start], this displays `<message>... <spinner>`. The spinner
/// will spin until [Spinner::finish] is called with the result of the operation.
///
/// When the [Spinner::finish] is called, the spinner will be replaced with either
/// ✓ or ✗, and in the event of an error, the error message will be printed on the
/// next line.
#[must_use]
pub struct Spinner<T, E> {
    tx: mpsc::SyncSender<Result<T, E>>,
    rx_return: mpsc::Receiver<Result<T, E>>,
}

impl<T, E> Spinner<T, E>
where
    T: Send + 'static,
    E: fmt::Display + 'static + Send + fmt::Debug,
{
    /// Starts a loading spinner with a message
    ///
    /// **NB** do not print anything while the spinner is running as this will corrupt the output
    pub fn start(title: impl Into<Cow<'static, str>>) -> Self {
        let (tx, rx) = mpsc::sync_channel(0);
        let (tx_return, rx_return) = mpsc::sync_channel(0);
        let title = title.into();
        std::thread::spawn(move || {
            Self::run_loop(title, rx, tx_return, |_err| true);
        });

        Self { tx, rx_return }
    }

    pub fn start_filter_errors(
        title: impl Into<Cow<'static, str>>,
        filter_err: impl Fn(&E) -> bool + Send + 'static,
    ) -> Self {
        let (tx, rx) = mpsc::sync_channel(0);
        let (tx_return, rx_return) = mpsc::sync_channel(0);
        let title = title.into();
        std::thread::spawn(move || {
            Self::run_loop(title, rx, tx_return, filter_err);
        });

        Self { tx, rx_return }
    }

    /// Replaces the spinner with a success/failure indication, and returns the result for
    /// further use
    pub fn finish(self, res: Result<T, E>) -> Result<T, Fancy<E>> {
        self.tx.send(res).unwrap();
        // Send another message and wait for this to fail due to the receiver being dropped
        // This means the log loop has exited and we are safe to start logging something else
        self.rx_return
            .recv()
            .unwrap()
            .map_err(|err| Fancy { err, _secret: () })
    }

    fn run_loop(
        title: Cow<'static, str>,
        rx_stop: mpsc::Receiver<Result<T, E>>,
        tx_return: mpsc::SyncSender<Result<T, E>>,
        filter_err: impl Fn(&E) -> bool,
    ) {
        const SPINNER: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

        let ms = Duration::from_millis(50);
        let mut count_ticks = 0;
        let mut stdout = std::io::stdout();
        let res = loop {
            let start_loop = Instant::now();
            if stdout.is_terminal() {
                write!(
                    stdout,
                    "{title}... {}\r",
                    SPINNER[count_ticks % SPINNER.len()]
                )
                .unwrap();
                stdout.flush().unwrap();
            }
            std::thread::sleep(ms.saturating_sub(start_loop.elapsed()));
            count_ticks += 1;

            match rx_stop.try_recv() {
                Ok(val) => break val,
                Err(mpsc::TryRecvError::Disconnected) => {
                    panic!("Spinner dropped without calling `finish`")
                }
                Err(mpsc::TryRecvError::Empty) => continue,
            }
        };

        match res.as_ref().err().filter(|&e| filter_err(e)) {
            None => {
                writeln!(stdout, "{title}... {}", "✓".green().bold()).unwrap();
                stdout.flush().unwrap();
            }
            Some(e) => {
                writeln!(stdout, "{title}... {}", "✗".red().bold(),).unwrap();
                stdout.flush().unwrap();
                error!("{e:#}");
            }
        }

        tx_return.send(res).unwrap();
    }
}

#[derive(thiserror::Error, Debug)]
#[error("{err:#}")]
/// A "fancy" error (which won't be logged upon exiting the program)
///
/// This is the error type returned by `Spinner`. It indicates that this error
/// has already been logged and therefore we shouldn't return the error from
/// `main`, we should just exit with code 1.
///
/// Using [impl_convert], you can create conversions which perform the relevant
/// logging. This ensures errors generated outside the relevant spinner are also
/// logged.
pub struct Fancy<E> {
    pub(crate) err: E,
    _secret: (),
}

#[derive(thiserror::Error, Debug)]
pub enum MaybeFancy<E> {
    #[error(transparent)]
    Unfancy(E),
    #[error(transparent)]
    Fancy(Fancy<E>),
}

impl<E> From<E> for MaybeFancy<E> {
    fn from(value: E) -> Self {
        Self::Unfancy(value)
    }
}

impl<E> From<Fancy<E>> for MaybeFancy<E> {
    fn from(value: Fancy<E>) -> Self {
        Self::Fancy(value)
    }
}

impl<E> Fancy<E> {
    pub fn convert<F: From<E>>(self) -> Fancy<F> {
        Fancy {
            err: self.err.into(),
            _secret: (),
        }
    }

    fn log(err: E) -> Self
    where
        E: fmt::Display,
    {
        error!("{err:#}");
        Self { err, _secret: () }
    }
}

macro_rules! impl_convert {
    (Fancy<$from:ty> => Fancy<$to:ty>) => {
        impl From<Fancy<$from>> for Fancy<$to> {
            fn from(err: Fancy<$from>) -> Self {
                err.convert()
            }
        }
    };
    ($from:ty => Fancy<$to:ty>) => {
        impl From<$from> for Fancy<$to> {
            fn from(err: $from) -> Self {
                Self::log(err.into())
            }
        }
    };
    (Fancy<$from:ty> => MaybeFancy<$to:ty>) => {
        impl From<Fancy<$from>> for MaybeFancy<$to> {
            fn from(err: Fancy<$from>) -> Self {
                MaybeFancy::Fancy(err.convert())
            }
        }
    };
    ($from:ty => MaybeFancy<$to:ty>) => {
        impl From<$from> for MaybeFancy<$to> {
            fn from(err: $from) -> Self {
                MaybeFancy::Unfancy(err.into())
            }
        }
    };
}

impl_convert!(ConnectError => Fancy<ConnectError>);
impl_convert!(SystemServiceError => Fancy<ConnectError>);
impl_convert!(anyhow::Error => Fancy<ConnectError>);
impl_convert!(MultiError => Fancy<ConnectError>);
impl_convert!(TEdgeError => Fancy<ConnectError>);
impl_convert!(CertError => MaybeFancy<anyhow::Error>);
impl_convert!(Fancy<anyhow::Error> => Fancy<ConnectError>);
impl_convert!(Fancy<std::io::Error> => Fancy<ConnectError>);
impl_convert!(SystemServiceError => Fancy<SystemServiceError>);
impl_convert!(Fancy<SystemServiceError> => Fancy<DisconnectBridgeError>);
impl_convert!(Fancy<anyhow::Error> => Fancy<DisconnectBridgeError>);
impl_convert!(Fancy<DisconnectBridgeError> => MaybeFancy<anyhow::Error>);
impl_convert!(Fancy<ConnectError> => MaybeFancy<anyhow::Error>);

/// A mechanism to log the current configuration, designed to aid debugging `tedge connect` issues
pub struct ConfigLogger<'a> {
    title: Cow<'static, str>,
    device_id: &'a str,
    cloud_host: String,
    cert_path: &'a Utf8Path,
    bridge_location: BridgeLocation,
    auth_method: Option<AuthMethod>,
    service_manager: &'a dyn SystemServiceManager,
    mosquitto_version: Option<&'a str>,
    cloud: &'a MaybeBorrowedCloud<'a>,
    credentials_path: Option<&'a Utf8Path>,
}

impl<'a> ConfigLogger<'a> {
    /// Print a summary of the bridge config to stdout
    pub fn log(
        title: impl Into<Cow<'static, str>>,
        config: &'a BridgeConfig,
        service_manager: &'a dyn SystemServiceManager,
        cloud: &'a MaybeBorrowedCloud<'a>,
        credentials_path: Option<&'a Utf8Path>,
    ) {
        println!(
            "{}",
            Self {
                title: title.into(),
                device_id: &config.remote_clientid,
                cloud_host: config.address.to_string(),
                cert_path: &config.bridge_certfile,
                bridge_location: config.bridge_location,
                auth_method: config.auth_method,
                credentials_path,
                service_manager,
                mosquitto_version: config.mosquitto_version.as_deref(),
                cloud,
            }
        )
    }

    fn log_single_entry(
        &self,
        f: &mut fmt::Formatter,
        name: &str,
        val: &dyn fmt::Display,
    ) -> fmt::Result {
        write!(
            f,
            "\n\t{} {}",
            format_args!("{name}:").blue().bold(),
            val.blue()
        )
    }
}

impl fmt::Display for ConfigLogger<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:", self.title)?;
        self.log_single_entry(f, "device id", &self.device_id)?;
        if let Some(profile) = self.cloud.profile_name() {
            self.log_single_entry(f, "cloud profile", profile)?;
        } else {
            self.log_single_entry(f, "cloud profile", &"<none>")?;
        }
        self.log_single_entry(f, "cloud host", &self.cloud_host)?;
        let mut auth_type = AuthType::Certificate;
        if let Some(auth_method) = self.auth_method {
            self.log_single_entry(f, "auth method", &auth_method)?;
            if let Some(path) = self.credentials_path {
                auth_type = auth_method.to_type(path);
                if AuthType::Basic == auth_type {
                    self.log_single_entry(f, "credentials path", &path)?
                }
            }
        }
        if AuthType::Certificate == auth_type {
            self.log_single_entry(f, "certificate file", &self.cert_path)?;
        }
        self.log_single_entry(f, "bridge", &self.bridge_location)?;
        self.log_single_entry(f, "service manager", &self.service_manager.name())?;
        if let Some(mosquitto_version) = self.mosquitto_version {
            self.log_single_entry(f, "mosquitto version", &mosquitto_version)?;
        }

        Ok(())
    }
}
