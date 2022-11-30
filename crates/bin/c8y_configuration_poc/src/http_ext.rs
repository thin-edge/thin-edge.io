use async_trait::async_trait;
use std::ptr::addr_of;
use tedge_actors::{
    new_mailbox, Actor, Address, ChannelError, Mailbox, Message, Recipient, RuntimeError,
    RuntimeHandle, Sender,
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HttpConfig {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HttpRequest {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HttpResponse {}

/// Create a new HTTP connection managed behind the scene by an actor
///
/// This connection is private,
/// i.e only the callee of `new_private_connection()` will be able to interact with it.
///
/// ```
///       client                    http_con
///             --------------------->|||| ============> http://host
///         ||||<---------------------
/// ```
pub async fn new_private_connection(
    runtime: &mut RuntimeHandle,
    config: HttpConfig,
    client: Recipient<HttpResponse>,
) -> Result<Recipient<HttpRequest>, RuntimeError> {
    let (mailbox, address) = new_mailbox(10);

    let actor = PrivateHttpActor::new(config);
    runtime.run(actor, mailbox, client).await?;

    Ok(address.as_recipient())
}

struct PrivateHttpActor {
    // Some HTTP connection to a remote server
}

impl PrivateHttpActor {
    fn new(_config: HttpConfig) -> Self {
        PrivateHttpActor {}
    }
}

#[async_trait]
impl Actor for PrivateHttpActor {
    type Input = HttpRequest;
    type Output = HttpResponse;
    type Mailbox = Mailbox<HttpRequest>;
    type Peers = Recipient<HttpResponse>;

    async fn run(
        self,
        mut requests: Self::Mailbox,
        mut client: Self::Peers,
    ) -> Result<(), ChannelError> {
        while let Some(_request) = requests.next().await {
            // Forward the request to the http server
            // Await for a response
            let response = HttpResponse {};

            // Send the response back to the client
            client.send(response).await?
        }

        Ok(())
    }
}

/// Create a new HTTP connection managed behind the scene by an actor
///
/// This connection can be shared by several clients,
pub struct HttpActorInstance {
    config: HttpConfig,
    mailbox: Mailbox<(usize, HttpRequest)>,
    address: Address<(usize, HttpRequest)>,
    clients: Vec<Recipient<HttpResponse>>,
}

impl HttpActorInstance {
    pub fn new(config: HttpConfig) -> Self {
        let (mailbox, address) = new_mailbox(10);

        HttpActorInstance {
            config,
            mailbox,
            address,
            clients: vec![],
        }
    }

    pub fn add_client(&mut self, client: Recipient<HttpResponse>) -> Recipient<HttpRequest> {
        let client_idx = self.clients.len();
        self.clients.push(client);

        KeyedRecipient::new(client_idx, self.address.clone())
    }

    pub async fn spawn(self, runtime: &mut RuntimeHandle) -> Result<(), RuntimeError> {
        let actor = HttpActor::new(self.config);
        let mailbox = self.mailbox;
        let clients = RecipientVec::new_recipient(self.clients);

        runtime.run(actor, mailbox, clients).await
    }
}

pub struct KeyedRecipient<M: Message> {
    idx: usize,
    address: Address<(usize, M)>,
}

impl<M: Message> KeyedRecipient<M> {
    pub fn new(idx: usize, address: Address<(usize, M)>) -> Recipient<M> {
        Box::new(KeyedRecipient { idx, address })
    }

    pub fn clone(&self) -> Recipient<M> {
        Box::new(KeyedRecipient {
            idx: self.idx,
            address: self.address.clone(),
        })
    }
}

#[async_trait]
impl<M: Message> Sender<M> for KeyedRecipient<M> {
    async fn send(&mut self, message: M) -> Result<(), ChannelError> {
        self.address.send((self.idx, message)).await
    }

    fn recipient_clone(&self) -> Recipient<M> {
        self.clone()
    }
}

struct HttpActor {
    // Some HTTP connection to a remote server
}

impl HttpActor {
    fn new(_config: HttpConfig) -> Self {
        HttpActor {}
    }
}

#[async_trait]
impl Actor for HttpActor {
    type Input = (usize, HttpRequest);
    type Output = (usize, HttpResponse);
    type Mailbox = Mailbox<Self::Input>;
    type Peers = Recipient<Self::Output>;

    async fn run(
        self,
        mut requests: Self::Mailbox,
        mut client: Self::Peers,
    ) -> Result<(), ChannelError> {
        while let Some((client_id, _request)) = requests.next().await {
            // Forward the request to the http server
            // Await for a response
            let response = HttpResponse {};

            // Send the response back to the client
            client.send((client_id, response)).await?
        }

        Ok(())
    }
}

struct RecipientVec<M: Message> {
    recipients: Vec<Recipient<M>>,
}

impl<M: Message> RecipientVec<M> {
    pub fn new_recipient(recipients: Vec<Recipient<M>>) -> Recipient<(usize, M)> {
        Box::new(RecipientVec { recipients })
    }
}

#[async_trait]
impl<M: Message> Sender<(usize, M)> for RecipientVec<M> {
    async fn send(&mut self, idx_message: (usize, M)) -> Result<(), ChannelError> {
        let (idx, message) = idx_message;
        if let Some(recipient) = self.recipients.get_mut(idx) {
            recipient.send(message).await?;
        }
        Ok(())
    }

    // TODO: Do we really need to clone recipients?
    //       This was useful when the address of the caller where packed along the request.
    //       But if we use pre-establish channels as done here, then cloning a recipient is useless.
    //       Getting rid of this clone method would make things a lot easier.
    //       Notably, one can have then `type Recipient<M> = Box<dyn Sink<M>>`
    //       and use `SinkExt::with()` to adapt recipients to the caller.
    fn recipient_clone(&self) -> Recipient<(usize, M)> {
        let recipients = self
            .recipients
            .iter()
            .map(|r| r.recipient_clone())
            .collect();
        Box::new(RecipientVec { recipients })
    }
}
