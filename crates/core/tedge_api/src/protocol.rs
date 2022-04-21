use crate::{Message, RuntimeError};
use async_trait::async_trait;
use futures::channel::mpsc;
use futures::{SinkExt, StreamExt};
use std::fmt::{Debug, Formatter};

/// A mailbox gathering all the messages to be processed by a plugin
pub struct MailBox<M> {
    sender: mpsc::UnboundedSender<M>,
    receiver: mpsc::UnboundedReceiver<M>,
}

impl<M> MailBox<M> {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::unbounded();
        MailBox { sender, receiver }
    }

    pub async fn next(&mut self) -> Option<M> {
        self.receiver.next().await
    }

    pub fn get_address(&self) -> Address<M> {
        Address {
            sender: self.sender.clone(),
        }
    }
}

/// A recipient for messages of type `M`
#[async_trait]
pub trait Recipient<M> {
    async fn send_msg(&mut self, message: M) -> Result<(), RuntimeError>;
}

/// An address where messages of type `M` can be sent
#[derive(Clone, Debug)]
pub struct Address<M> {
    sender: mpsc::UnboundedSender<M>,
}

#[async_trait]
impl<M: Message, N: Message + Into<M>> Recipient<N> for Address<M> {
    async fn send_msg(&mut self, message: N) -> Result<(), RuntimeError> {
        Ok(self.sender.send(message.into()).await?)
    }
}

/// A request which response has to be sent to a given recipient
pub struct Request<Req, Res> {
    request: Req,
    requester: Box<dyn Recipient<Res>>,
}

impl<Req: Message, Res> Debug for Request<Req, Res> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("Request({:?})", self.request))
    }
}

impl<M: Message> Address<M> {
    /// Send a request which response will be sent to this address
    pub async fn send_request_to<Req, Res: Message + Into<M>>(
        &self,
        recipient: &mut impl Recipient<Request<Req, Res>>,
        request: Req,
    ) -> Result<(), RuntimeError> {
        let requester: Box<dyn Recipient<Res>> = Box::new(self.clone());
        recipient.send_msg(Request { request, requester }).await
    }
}

/// The actual request of a `Request` struct
impl<Req, Res> AsRef<Req> for Request<Req, Res> {
    fn as_ref(&self) -> &Req {
        &self.request
    }
}

impl<Req, Res> Request<Req, Res> {
    /// Send the response for a request to the requester
    pub async fn send_response(mut self, response: Res) -> Result<(), RuntimeError> {
        self.requester.send_msg(response).await
    }
}

/// A plugin that produces messages
pub trait Producer<M> {
    /// Connect this producer to a recipient that will receive all the produced messages
    fn add_recipient(&mut self, recipient: Address<M>);
}

/// A plugin that sends requests
pub trait Requester<Req, Res> {
    /// Connect this requester to a recipient that will respond to the requests.
    fn add_responder(&mut self, recipient: Address<Request<Req, Res>>);
}
