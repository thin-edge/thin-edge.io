use async_trait::async_trait;
use futures::{future::FutureExt, stream::StreamExt};
use signals::*;
use thiserror::Error;
use tokio::select;

mod signals;

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
/// The life of a service begins with its `setup`, shortly followed by
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
/// ```ignore
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
/// The `run` method runs concurrently with the signal handlers (this
/// applies to any `async` functions scheduled on the same scheduler
/// thread). That means, if you have a busy loop in `run` or you do not
/// give up control from `run` to the tokio scheduler (e.g. by means of
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
    async fn reload(self) -> Result<Self, Self::Error>;

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
    signal_builder: SignalStreamBuilder,
}

impl<S: Service> ServiceRunner<S> {
    pub fn new() -> Self {
        Self {
            _marker: std::marker::PhantomData,
            signal_builder: SignalStreamBuilder::new(),
        }
    }

    pub fn ignore_sighup(self) -> Self {
        Self {
            signal_builder: self.signal_builder.ignore_sighup(),
            ..self
        }
    }

    pub fn ignore_sigterm(self) -> Self {
        Self {
            signal_builder: self.signal_builder.ignore_sigterm(),
            ..self
        }
    }

    pub fn ignore_sigint(self) -> Self {
        Self {
            signal_builder: self.signal_builder.ignore_sigint(),
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
        let mut signal_stream = self.signal_builder.build()?;

        loop {
            let run_result = Self::run_service_with_signals(&mut service, &mut signal_stream).await;

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
                    service = service.reload().await.map_err(ServiceError::ServiceError)?;
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
        service: &mut S,
        signal_stream: &mut SignalStream,
    ) -> Result<ExitReason, ServiceError<S::Error>> {
        let mut service_fut = service.run().fuse();

        loop {
            select! {
                service_result = &mut service_fut => {
                    log::info!("Service terminated with: {:?}", service_result);
                    return service_result.map(|()| ExitReason::Terminate).map_err(ServiceError::ServiceError);
                }

                signal = signal_stream.next() => {
                    match signal.ok_or(ServiceError::SignalStreamExhausted)? {
                        SignalKind::Terminate => {
                            log::info!("Got SIGTERM");
                            return Ok(ExitReason::Terminate);
                        }
                        SignalKind::Interrupt => {
                            log::info!("Got SIGINT");
                            return Ok(ExitReason::Terminate);
                        }
                        SignalKind::Hangup => {
                            log::info!("Got SIGHUP");
                            return Ok(ExitReason::ReloadConfig);
                        }
                    }
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
