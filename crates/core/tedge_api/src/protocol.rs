use futures::channel::mpsc;
use futures::{SinkExt, StreamExt};
use crate::RuntimeError;

/// The mailbox gathering all the messages to be processed by a plugin
pub struct MailBox<M> {
    sender: mpsc::UnboundedSender<M>,
    receiver: mpsc::UnboundedReceiver<M>,
}

impl<M> MailBox<M> {
    pub fn new() -> Self {
        let (sender,receiver) = mpsc::unbounded();
        MailBox { sender, receiver }
    }

    pub async fn next(&mut self) -> Option<M> {
        self.receiver.next().await
    }

    pub fn get_address(&self) -> Address<M> {
        Address{ recipient: self.sender.clone() }
    }
}

/// An address where messages of type `M` can be sent
pub struct Address<M> {
    recipient: mpsc::UnboundedSender<M>,
}

impl<M> Address<M> {

    /// Send a message to this address
    pub async fn send(&mut self, message: impl Into<M>) -> Result<(), RuntimeError> {
        Ok(self.recipient.send(message.into()).await?)
    }

    /// Send a request which response will be sent to this address
    pub async fn send_request_to<Req>(self, recipient: &mut Address<Request<Req,M>>, request: Req) -> Result<(), RuntimeError> {
        recipient.send(Request { request, requester: self }).await
    }
}

/// A request which response has to be sent to a given address
pub struct Request<Req,Res> {
    request: Req,
    requester: Address<Res>,
}

/// The actual request of a `Request` struct
impl<Req,Res> AsRef<Req> for Request<Req,Res> {
    fn as_ref(&self) -> &Req {
        &self.request
    }
}

impl<Req,Res> Request<Req,Res> {

    /// Send the response for a request to the requester
    pub async fn send_response(mut self, response: impl Into<Res>) -> Result<(), RuntimeError> {
        self.requester.send(response).await
    }
}

/// A plugin that produces messages
pub trait Producer<M> {

    /// Connect this producer to a recipient that will receive all the produced messages
    fn add_recipient(&mut self, recipient: Address<M>);
}

/// A plugin that sends requests
pub trait Requester<Req,Res> {

    /// Connect this requester to a recipient that will respond to the requests.
    fn add_responder(&mut self, recipient: Address<Request<Req,Res>>);
}
