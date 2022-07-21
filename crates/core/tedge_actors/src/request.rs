use crate::{Message, Recipient, RuntimeError};
use futures::channel::oneshot;

/// A request of type `Req` awaiting a response of type `Res`
///
/// A request struct encapsulates the actual request with information about the requester
#[derive(Debug)]
pub struct Request<Req: Message, Res: Message> {
    request: Req,
    response_sender: oneshot::Sender<Res>,
}

/// A Request is a Message
impl<Req: Message, Res: Message> Message for Request<Req, Res> {}

/// A Request envelop can be directly used as a request
impl<Req: Message, Res: Message> AsRef<Req> for Request<Req, Res> {
    fn as_ref(&self) -> &Req {
        &self.request
    }
}

/// Send a request of type `Req` to a recipient of such requests
pub async fn send_request<Req: Message, Res: Message>(
    mut actor: Recipient<Request<Req, Res>>,
    request: Req,
) -> Result<Res, RuntimeError> {
    let (response_sender, response_receiver) = oneshot::channel();
    let request = Request {
        request,
        response_sender,
    };
    let () = actor.send_message(request).await?;
    Ok(response_receiver.await?)
}

impl<Req: Message, Res: Message> Request<Req, Res> {
    /// Send a response for that request
    pub async fn send_response(self, response: Res) -> Result<(), RuntimeError> {
        self.response_sender
            .send(response)
            .map_err(|_| RuntimeError::Canceled(futures::channel::oneshot::Canceled))
    }
}
