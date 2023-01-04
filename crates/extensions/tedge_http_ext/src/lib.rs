mod actor;
mod handle;
mod messages;

pub use handle::*;
pub use messages::*;

use actor::*;
use async_trait::async_trait;
use futures::channel::mpsc;
use tedge_actors::ActorBuilder;
use tedge_actors::DynSender;
use tedge_actors::KeyedSender;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeHandle;
use tedge_actors::SenderVec;

pub trait HttpConnectionBuilder {
    fn connect(&mut self, client: DynSender<HttpResult>) -> DynSender<HttpRequest>;

    fn new_handle(&mut self) -> HttpHandle {
        HttpHandle::new(self)
    }
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

impl HttpConnectionBuilder for HttpActorBuilder {
    fn connect(&mut self, client: DynSender<HttpResult>) -> DynSender<HttpRequest> {
        let client_idx = self.clients.len();
        self.clients.push(client);

        KeyedSender::new_sender(client_idx, self.sender.clone())
    }
}
