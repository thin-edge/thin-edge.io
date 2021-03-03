use async_trait::async_trait;
pub use signals::*;
use thiserror::Error;

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
/// * Service shutdown (`Service#shutdown`)
///
/// The life of a service begins with its `setup`, shortly followed by
/// invoking its service handling loop `run`. A `SignalStream` is passed
/// to the `run` method in order to react on incoming signals like SIGHUP,
/// SIGTERM or SIGINT. The `shutdown` method is called once `run` has
/// terminated, independent of wether or not `run` returned `Ok` or `Err`.
/// This is where to put code to gracefully shutdown the service.
///
/// ```ignore
///     +---------------+
///     |     setup     |
///     +---------------+
///             |
///             |
///             v
///     +---------------+
///     |      run      |
///     +---------------+
///             |
///             |
///             v
///     +---------------+
///     |    shutdown   |
///     +---------------+
/// ```
///
#[async_trait]
pub trait Service: Sized + Send + 'static {
    /// The service name
    const NAME: &'static str;

    /// Associated error
    type Error: std::error::Error + 'static;

    /// The configuration type passed to `setup`
    type Configuration;

    /// Builds the service from `config` and initializes it to be ready for `run`ning.
    async fn setup(config: Self::Configuration) -> Result<Self, Self::Error>;

    /// Runs the service.
    async fn run(&mut self, signal_stream: SignalStream) -> Result<(), Self::Error>;

    /// Shuts the service down.
    async fn shutdown(self) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[derive(Error, Debug)]
pub enum ServiceError<E: std::error::Error + 'static> {
    #[error("Service error: {0}")]
    ServiceError(E),

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
        let signal_stream = self.signal_builder.build()?;
        let mut service = S::setup(config).await.map_err(ServiceError::ServiceError)?;
        let run_result = service
            .run(signal_stream)
            .await
            .map_err(ServiceError::ServiceError);
        log::info!("Service {} stopped running with: {:?}", S::NAME, run_result);
        log::info!("Shutting down service {}", S::NAME);
        if let Err(shutdown_err) = service.shutdown().await {
            log::warn!(
                "Shutdown of service {} failed with: {:?}",
                S::NAME,
                shutdown_err
            );
        }
        run_result
    }
}
