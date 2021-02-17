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

    /// Creates the service
    async fn create(config: Self::Configuration) -> Result<Self, Self::Error>;

    /// Runs the service.
    async fn run(&mut self) -> Result<(), Self::Error>;

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
}

impl<S: Service> ServiceRunner<S> {
    pub fn new() -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }

    pub async fn run_with_config(
        self,
        load_config: impl Fn() -> Result<S::Configuration, S::Error>,
    ) -> Result<(), ServiceError<S::Error>> {
        loop {
            log::debug!("Load configuration");
            let config = load_config().map_err(ServiceError::ServiceError)?;
            let mut service = S::create(config)
                .await
                .map_err(ServiceError::ServiceError)?;
            let run_result = Self::run_service_with_signals(&mut service).await;
            let () = service
                .shutdown()
                .await
                .map_err(ServiceError::ServiceError)?;
            match run_result? {
                ExitReason::Terminate => {
                    return Ok(());
                }
                ExitReason::ReloadConfig => {
                    log::debug!("Reload config");
                    continue;
                }
            }
        }
    }

    async fn run_service_with_signals(
        service: &mut S,
    ) -> Result<ExitReason, ServiceError<S::Error>> {
        let mut hangup_signals = signal(SignalKind::hangup())?;
        let mut terminate_signals = signal(SignalKind::terminate())?;
        let mut interrupt_signals = signal(SignalKind::interrupt())?;

        let service_fut = service.run().fuse();

        select! {
            service_result = service_fut => {
                log::debug!("Service terminated with: {:?}", service_result);
                return service_result.map(|()| ExitReason::Terminate).map_err(ServiceError::ServiceError);
            }

            terminate_signal = terminate_signals.recv() => {
                log::debug!("Got SIGTERM");
                let () = terminate_signal.ok_or(ServiceError::SignalStreamExhausted)?;
                return Ok(ExitReason::Terminate);
            }

            interrupt_signal = interrupt_signals.recv() => {
                log::debug!("Got SIGINT");
                let () = interrupt_signal.ok_or(ServiceError::SignalStreamExhausted)?;
                return Ok(ExitReason::Terminate);
            }

            hangup_signal = hangup_signals.recv() => {
                log::debug!("Got SIGHUP");
                let () = hangup_signal.ok_or(ServiceError::SignalStreamExhausted)?;
                return Ok(ExitReason::ReloadConfig);
            }
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum ExitReason {
    ReloadConfig,
    Terminate,
}
