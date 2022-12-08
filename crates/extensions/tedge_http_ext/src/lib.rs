mod actor;
mod messages;

pub use messages::*;

use actor::*;
use async_trait::async_trait;
use tedge_actors::{new_mailbox, ActorInstance, Address, Mailbox, Recipient, RuntimeError, RuntimeHandle, KeyedRecipient, RecipientVec};

pub struct HttpActorInstance {
    actor: HttpActor,
    mailbox: Mailbox<(usize, HttpRequest)>,
    address: Address<(usize, HttpRequest)>,
    clients: Vec<Recipient<Result<HttpResponse, HttpError>>>,
}

impl HttpActorInstance {
    pub fn new(config: HttpConfig) -> Result<Self, HttpError> {
        let actor = HttpActor::new(config)?;
        let (mailbox, address) = new_mailbox(10);

        Ok(HttpActorInstance {
            actor,
            mailbox,
            address,
            clients: vec![],
        })
    }

    pub fn add_client(
        &mut self,
        client: Recipient<Result<HttpResponse, HttpError>>,
    ) -> Recipient<HttpRequest> {
        let client_idx = self.clients.len();
        self.clients.push(client);

        KeyedRecipient::new_recipient(client_idx, self.address.clone())
    }
}

#[async_trait]
impl ActorInstance for HttpActorInstance {
    async fn spawn(self, runtime: &mut RuntimeHandle) -> Result<(), RuntimeError> {
        let actor = self.actor;
        let mailbox = self.mailbox;
        let clients = RecipientVec::new_recipient(self.clients);

        runtime.run(actor, mailbox, clients).await
    }
}
