mod actor;
mod messages;

pub use messages::*;
use std::convert::Infallible;

use actor::*;
use async_trait::async_trait;
use tedge_actors::mpsc;
use tedge_actors::ActorBuilder;
use tedge_actors::ConnectionBuilder;
use tedge_actors::DynSender;
use tedge_actors::KeyedSender;
use tedge_actors::RequestResponseHandler;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeHandle;
use tedge_actors::SenderVec;

pub type HttpHandle = RequestResponseHandler<HttpRequest, HttpResult>;
pub trait HttpConnectionBuilder:
    ConnectionBuilder<HttpRequest, HttpResult, (), Infallible>
{
}

pub struct HttpActorBuilder {
    actor: HttpActor,
    receiver: mpsc::Receiver<(usize, HttpRequest)>,
    sender: mpsc::Sender<(usize, HttpRequest)>,
    clients: Vec<DynSender<Result<HttpResponse, HttpError>>>,
}

impl HttpActorBuilder {
    pub fn new(config: HttpConfig) -> Result<Self, HttpError> {
        let actor = HttpActor::new(config)?;
        let (sender, receiver) = mpsc::channel(10);

        Ok(HttpActorBuilder {
            actor,
            receiver,
            sender,
            clients: vec![],
        })
    }
}

#[async_trait]
impl ActorBuilder for HttpActorBuilder {
    async fn spawn(self, runtime: &mut RuntimeHandle) -> Result<(), RuntimeError> {
        let actor = self.actor;
        let request_receiver = self.receiver;
        let response_sender = SenderVec::new_sender(self.clients);
        let messages = HttpMessageBox::new(4, request_receiver, response_sender);
        runtime.run(actor, messages).await
    }
}

impl ConnectionBuilder<HttpRequest, HttpResult, (), Infallible> for HttpActorBuilder {
    fn connect(
        &mut self,
        _config: (),
        client: DynSender<HttpResult>,
    ) -> Result<DynSender<HttpRequest>, Infallible> {
        let client_idx = self.clients.len();
        self.clients.push(client);

        Ok(KeyedSender::new_sender(client_idx, self.sender.clone()))
    }
}

impl HttpConnectionBuilder for HttpActorBuilder {}
