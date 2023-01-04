use crate::mpsc;
use crate::DynSender;
use crate::Message;
use crate::RequestResponseHandler;
use crate::RuntimeError;
use crate::RuntimeHandle;
use async_trait::async_trait;

/// Materialize an actor instance under construction
///
/// Such an instance is:
/// 1. built from some actor configuration
/// 2. connected to other peers
/// 3. eventually spawned into an actor.
#[async_trait]
pub trait ActorBuilder {
    /// Build and spawn the actor
    async fn spawn(self, runtime: &mut RuntimeHandle) -> Result<(), RuntimeError>;
}

/// Implemented by actor builders to connect the actors under construction
pub trait ConnectionBuilder<Input: Message, Output: Message, Config, Error: std::error::Error> {
    fn connect(
        &mut self,
        config: Config,
        output_sender: DynSender<Output>,
    ) -> Result<DynSender<Input>, Error>;

    /// Create a new request/response connection handle
    ///
    /// Panic if the connection cannot be created,
    /// either because the config is ill-formed
    /// or because the service cannot accept a new connection with such a config.
    fn new_request_handle(&mut self, config: Config) -> RequestResponseHandler<Input, Output> {
        self.try_new_request_handle(config)
            .expect("Fail to connect a new client")
    }

    fn try_new_request_handle(
        &mut self,
        config: Config,
    ) -> Result<RequestResponseHandler<Input, Output>, Error> {
        // At most one response is expected
        let (response_sender, response_receiver) = mpsc::channel(1);

        let request_sender = self.connect(config, response_sender.into())?;
        Ok(RequestResponseHandler {
            request_sender,
            response_receiver,
        })
    }
}
