use async_trait::async_trait;
use futures::future::FutureExt;
use thiserror::Error;
use tokio::{
    select,
    signal::unix::{signal, SignalKind},
};

#[async_trait]
pub trait Service: Sized {
    /// The service name
    const NAME: &'static str;

    /// Associated error
    type Error: std::error::Error + 'static;

    /// The configuration type passed to `create`
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

    pub fn ignore_sighup(self) -> Self {
        Self {
            ignore_sighup: true,
            ..self
        }
    }

    pub fn ignore_sigterm(self) -> Self {
        Self {
            ignore_sigterm: true,
            ..self
        }
    }

    pub fn ignore_sigint(self) -> Self {
        Self {
            ignore_sigint: true,
            ..self
        }
    }

    pub async fn run_with_config(
        self,
        config: S::Configuration,
    ) -> Result<(), ServiceError<S::Error>> {
        log::info!("Running service {}. pid={}", S::NAME, std::process::id());
        let mut service = S::setup(config).await.map_err(ServiceError::ServiceError)?;

        loop {
            let run_result = self.run_service_with_signals(&mut service).await;

            match run_result {
                Ok(ExitReason::Terminate) => {
                    log::debug!("Shutting service down");
                    let () = service
                        .shutdown()
                        .await
                        .map_err(ServiceError::ServiceError)?;
                    return Ok(());
                }
                Ok(ExitReason::ReloadConfig) => {
                    log::debug!("Reload config");
                    let () = service.reload().await.map_err(ServiceError::ServiceError)?;
                }
                Err(err) => {
                    log::debug!("Service failed with: {:?}", err);
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
        let mut hangup_signals = signal(SignalKind::hangup())?;
        let mut terminate_signals = signal(SignalKind::terminate())?;
        let mut interrupt_signals = signal(SignalKind::interrupt())?;

        let mut service_fut = service.run().fuse();

        loop {
            select! {
                service_result = &mut service_fut => {
                    log::debug!("Service terminated with: {:?}", service_result);
                    return service_result.map(|()| ExitReason::Terminate).map_err(ServiceError::ServiceError);
                }

                terminate_signal = terminate_signals.recv() => {
                    log::debug!("Got SIGTERM");
                    if self.ignore_sigterm {
                        continue;
                    }
                    let () = terminate_signal.ok_or(ServiceError::SignalStreamExhausted)?;
                    return Ok(ExitReason::Terminate);
                }

                interrupt_signal = interrupt_signals.recv() => {
                    log::debug!("Got SIGINT");
                    if self.ignore_sigint {
                        continue;
                    }
                    let () = interrupt_signal.ok_or(ServiceError::SignalStreamExhausted)?;
                    return Ok(ExitReason::Terminate);
                }

                hangup_signal = hangup_signals.recv() => {
                    log::debug!("Got SIGHUP");
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
