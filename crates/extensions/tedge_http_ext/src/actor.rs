use crate::{HttpConfig, HttpError, HttpRequest, HttpResponse};
use async_trait::async_trait;
use tedge_actors::{Actor, Address, ChannelError, Mailbox, Message, Recipient, Sender};

pub(crate) struct HttpActor {
    client: reqwest::Client,
}

impl HttpActor {
    pub(crate) fn new(_config: HttpConfig) -> Result<Self, HttpError> {
        let client = reqwest::Client::builder().build()?;
        Ok(HttpActor { client })
    }
}

#[async_trait]
impl Actor for HttpActor {
    type Input = (usize, HttpRequest);
    type Output = (usize, Result<HttpResponse, HttpError>);
    type Mailbox = Mailbox<Self::Input>;
    type Peers = Recipient<Self::Output>;

    async fn run(
        self,
        mut requests: Self::Mailbox,
        mut client: Self::Peers,
    ) -> Result<(), ChannelError> {
        while let Some((client_id, request)) = requests.next().await {
            // Forward the request to the http server
            let request = request.into();

            // Await for a response
            let response = match self.client.execute(request).await {
                Ok(res) => Ok(res.into()),
                Err(err) => Err(err.into()),
            };

            // Send the response back to the client
            client.send((client_id, response)).await?
        }

        Ok(())
    }
}

/// A recipient that adds a source id on the fly
pub struct KeyedRecipient<M: Message> {
    idx: usize,
    address: Address<(usize, M)>,
}

impl<M: Message> KeyedRecipient<M> {
    pub fn new_recipient(idx: usize, address: Address<(usize, M)>) -> Recipient<M> {
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

/// A vector of recipients to which messages are specifically addressed using a source id
pub struct RecipientVec<M: Message> {
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
