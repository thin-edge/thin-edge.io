use crate::signals;
use async_trait::async_trait;
use futures::future::FutureExt;
use thiserror::Error;
use tokio::select;

///
/// Service abstraction and integration with signal handling.
///
/// The `Service` trait provides a template for dealing with common
/// lifecycle events of a service.
///
/// # Service lifecycle
///
/// Currently, the following lifecycle events of a service exist:
///
/// * Service creation and initialization (`Service::setup`)
/// * Service execution (`Service#run`)
/// * Reloading (`Service#reload`)
/// * Service shutdown (`Service#shutdown`)
///
/// The life of a service begins with it's `setup`, shortly followed by
/// invoking it's service handling loop `run`. A `SIGHUP` signal, unless
/// ignored, will break out of `run` and enter `reload`. Once `reload`
/// is done, `run` is called again. It's important to note that anything
/// that you allocate on the "stack" within `run` will be dropped before
/// calling `reload`. If there is anything that you want to persist
/// across calls between `run` and `reload` that will have to go into
/// `setup`. The `SIGINT` or `SIGTERM` signals will as well break out of
/// `run`, drop anything allocated on the "stack frame" of `run`, and
/// then call `shutdown`. This is where resources can be deallocated.
///
/// ```
///     +---------------+
///     |     setup     |
///     +---------------+
///             |
///             |
///             v
///     +---------------+         +---------------+
///     |      run      |<------->|     reload    |
///     +---------------+         +---------------+
///             |
///             |
///             v
///     +---------------+
///     |    shutdown   |
///     +---------------+
/// ```
///
/// # Caveats
///
/// The `run` method runs concurrently with the signal handlers. That
/// means, if you have a busy loop in `run` or you do not give up
/// control from `run` to the tokio scheduler (e.g. by means of
/// `.await`ing), there is no chance for the signal handlers to run.
/// Signals will not be lost, but signal handling will be postponed to
/// when `run` gives up control.
///
#[async_trait]
pub trait Service: Sized {
    /// The service name
    const NAME: &'static str;

    /// Associated error
    type Error: std::error::Error + 'static;

    /// The configuration type passed to `setup`
    type Configuration;

    /// Builds the service from `config` and initializes it to be ready for `run`ning.
    async fn setup(config: Self::Configuration) -> Result<Self, Self::Error>;

    /// Runs the main loop of the service.
    async fn run(&mut self) -> Result<(), Self::Error>;

    /// Reloads the service.
    async fn reload(&mut self) -> Result<(), Self::Error>;

    /// Shuts the service down (gracefully).
    async fn shutdown(self) -> Result<(), Self::Error>;
}

#[derive(Error, Debug)]
pub enum ServiceError<E: std::error::Error + 'static> {
    #[error("Service error: {0}")]
    ServiceError(E),

    #[error("Signal stream exhausted")]
    SignalStreamExhausted,

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

pub struct ServiceRunner<S: Service> {
    _marker: std::marker::PhantomData<S>,
    ignore_sighup: bool,
    ignore_sigterm: bool,
    ignore_sigint: bool,
}

impl<S: Service> ServiceRunner<S> {
    pub fn new() -> Self {
        Self {
            _marker: std::marker::PhantomData,
            ignore_sighup: false,
            ignore_sigterm: false,
            ignore_sigint: false,
        }
    }

    #[allow(dead_code)]
    pub fn ignore_sighup(self) -> Self {
        Self {
            ignore_sighup: true,
            ..self
        }
    }

    #[allow(dead_code)]
    pub fn ignore_sigterm(self) -> Self {
        Self {
            ignore_sigterm: true,
            ..self
        }
    }

    #[allow(dead_code)]
    pub fn ignore_sigint(self) -> Self {
        Self {
            ignore_sigint: true,
            ..self
        }
    }

    pub async fn run_with_default_config(self) -> Result<(), ServiceError<S::Error>>
    where
        S::Configuration: Default,
    {
        self.run_with_config(S::Configuration::default()).await
    }

    pub async fn run_with_config(
        self,
        config: S::Configuration,
    ) -> Result<(), ServiceError<S::Error>> {
        log::info!("{} starting. pid={}", S::NAME, std::process::id());
        let mut service = S::setup(config).await.map_err(ServiceError::ServiceError)?;

        loop {
            let run_result = self.run_service_with_signals(&mut service).await;

            match run_result {
                Ok(ExitReason::Terminate) => {
                    log::info!("Shutting service down");
                    let () = service
                        .shutdown()
                        .await
                        .map_err(ServiceError::ServiceError)?;
                    return Ok(());
                }
                Ok(ExitReason::ReloadConfig) => {
                    log::info!("Reload config");
                    let () = service.reload().await.map_err(ServiceError::ServiceError)?;
                }
                Err(err) => {
                    log::info!("Service failed with: {:?}", err);
                    let () = service
                        .shutdown()
                        .await
                        .map_err(ServiceError::ServiceError)?;
                    return Err(err);
                }
            }
        }
    }

    async fn run_service_with_signals(
        &self,
        service: &mut S,
    ) -> Result<ExitReason, ServiceError<S::Error>> {
        let mut hangup_signals = signals::sighup_stream()?;
        let mut terminate_signals = signals::sigterm_stream()?;
        let mut interrupt_signals = signals::sigint_stream()?;

        let mut service_fut = service.run().fuse();

        loop {
            select! {
                service_result = &mut service_fut => {
                    log::info!("Service terminated with: {:?}", service_result);
                    return service_result.map(|()| ExitReason::Terminate).map_err(ServiceError::ServiceError);
                }

                terminate_signal = terminate_signals.recv() => {
                    log::info!("Got SIGTERM");
                    if self.ignore_sigterm {
                        continue;
                    }
                    let () = terminate_signal.ok_or(ServiceError::SignalStreamExhausted)?;
                    return Ok(ExitReason::Terminate);
                }

                interrupt_signal = interrupt_signals.recv() => {
                    log::info!("Got SIGINT");
                    if self.ignore_sigint {
                        continue;
                    }
                    let () = interrupt_signal.ok_or(ServiceError::SignalStreamExhausted)?;
                    return Ok(ExitReason::Terminate);
                }

                hangup_signal = hangup_signals.recv() => {
                    log::info!("Got SIGHUP");
                    if self.ignore_sighup {
                        continue;
                    }
                    let () = hangup_signal.ok_or(ServiceError::SignalStreamExhausted)?;
                    return Ok(ExitReason::ReloadConfig);
                }
            }
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum ExitReason {
    ReloadConfig,
    Terminate,
}
