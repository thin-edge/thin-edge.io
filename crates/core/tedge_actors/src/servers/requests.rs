use crate::Actor;
use crate::ChannelError;
use crate::DynSender;
use crate::LoggingReceiver;
use crate::Message;
use crate::MessageReceiver;
use crate::RuntimeError;
use crate::RuntimeRequest;
use crate::Sender;
use crate::Server;
use async_trait::async_trait;
use futures::channel::oneshot;
use futures::StreamExt;
use log::error;
use std::fmt::Debug;
use std::ops::ControlFlow;

/// Wrap a request with a [Sender] to send the response to
///
/// Requests are sent to server actors using such envelopes telling where to send the responses.
pub struct RequestEnvelope<Request, Response> {
    pub request: Request,
    pub reply_to: Box<dyn Sender<Response>>,
}

impl<Request: Debug, Response> Debug for RequestEnvelope<Request, Response> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.request.fmt(f)
    }
}

/// A message box used by a client to request a server and await the responses.
pub struct ClientMessageBox<Request, Response> {
    sender: DynSender<RequestEnvelope<Request, Response>>,
}

impl<Request: Message, Response: Message> ClientMessageBox<Request, Response> {
    /// Send the request and await for a response
    pub async fn await_response(&mut self, request: Request) -> Result<Response, ChannelError> {
        let (sender, receiver) = oneshot::channel::<Response>();
        let reply_to = Box::new(Some(sender));
        self.sender
            .send(RequestEnvelope { request, reply_to })
            .await?;
        let response = receiver.await;
        response.map_err(|_| ChannelError::ReceiveError())
    }
}

/// A [Sender] used by a client to send requests to a server,
/// redirecting the responses to another recipient.
#[derive(Clone)]
pub struct RequestSender<Request: 'static, Response: 'static> {
    sender: DynSender<RequestEnvelope<Request, Response>>,
    reply_to: DynSender<Response>,
}

#[async_trait]
impl<Request: Message, Response: Message> Sender<Request> for RequestSender<Request, Response> {
    async fn send(&mut self, request: Request) -> Result<(), ChannelError> {
        let reply_to = self.reply_to.sender();
        self.sender
            .send(RequestEnvelope { request, reply_to })
            .await
    }
}

/* Adding this prevents to derive Clone for RequestSender! Why?
impl<Request: Message, Response: Message> From<RequestSender<Request,Response>> for DynSender<Request> {
    fn from(sender: RequestSender<Request,Response>) -> Self {
        Box::new(sender)
    }
}*/

/// An actor that wraps a request-response server
///
/// Requests are processed in turn, leading either to a response or an error.
pub struct ServerActor<S: Server> {
    server: S,
    requests: LoggingReceiver<RequestEnvelope<S::Request, S::Response>>,
}

#[async_trait]
impl<S: Server> Actor for ServerActor<S> {
    fn name(&self) -> &str {
        self.server.name()
    }

    async fn run(mut self) -> Result<(), RuntimeError> {
        let server = &mut self.server;
        while let Some(RequestEnvelope {
            request,
            mut reply_to,
        }) = self.requests.recv().await
        {
            tokio::select! {
                response = server.handle(request) => {
                    let _ = reply_to.send(response).await;
                }
                Some(RuntimeRequest::Shutdown) = self.requests.recv_signal() => {
                    break;
                }
            }
        }
        Ok(())
    }
}

/// An actor that wraps a request-response protocol
///
/// Requests are processed concurrently (up to some max concurrency level).
///
/// The server must be `Clone` to create a fresh server handle for each request.
pub struct ConcurrentServerActor<S: Server + Clone> {
    server: S,
    messages: ConcurrentServerMessageBox<S::Request, S::Response>,
}

impl<S: Server + Clone> ConcurrentServerActor<S> {
    pub fn new(server: S, messages: ConcurrentServerMessageBox<S::Request, S::Response>) -> Self {
        ConcurrentServerActor { server, messages }
    }
}

#[async_trait]
impl<S: Server + Clone> Actor for ConcurrentServerActor<S> {
    fn name(&self) -> &str {
        self.server.name()
    }

    async fn run(mut self) -> Result<(), RuntimeError> {
        while let Some(RequestEnvelope {
            request,
            mut reply_to,
        }) = self.messages.next_request().await
        {
            // Spawn the request
            let mut server = self.server.clone();
            let pending_result = tokio::spawn(async move {
                let result = server.handle(request).await;
                let _ = reply_to.send(result).await;
            });

            // Send the response back to the client
            self.messages.send_response_once_done(pending_result)
        }

        Ok(())
    }
}

/// A message box for services that handles requests concurrently
pub struct ConcurrentServerMessageBox<Request: Debug, Response> {
    /// Max concurrent requests
    max_concurrency: usize,

    /// Message box to interact with clients of this service
    requests: LoggingReceiver<RequestEnvelope<Request, Response>>,

    /// Pending responses
    pending_responses: futures::stream::FuturesUnordered<PendingResult>,
}

type PendingResult = tokio::task::JoinHandle<()>;

impl<Request: Message, Response: Message> ConcurrentServerMessageBox<Request, Response> {
    pub(crate) fn new(
        max_concurrency: usize,
        requests: LoggingReceiver<RequestEnvelope<Request, Response>>,
    ) -> Self {
        ConcurrentServerMessageBox {
            max_concurrency,
            requests,
            pending_responses: futures::stream::FuturesUnordered::new(),
        }
    }

    async fn next_request(&mut self) -> Option<RequestEnvelope<Request, Response>> {
        if self.await_idle_processor().await.is_break() {
            return None;
        }

        loop {
            tokio::select! {
                Some(request) = self.requests.recv() => {
                    return Some(request);
                }
                Some(result) = self.pending_responses.next() => {
                    if let Err(err) = result {
                        error!("Request failed with: {err}");
                    }
                }
                else => {
                    return None
                }
            }
        }
    }

    async fn await_idle_processor(&mut self) -> ControlFlow<(), ()> {
        if self.pending_responses.len() < self.max_concurrency {
            return ControlFlow::Continue(());
        }

        tokio::select! {
            Some(result) = self.pending_responses.next() => {
                if let Err(err) = result {
                    error!("Request failed with: {err}");
                }
                ControlFlow::Continue(())
            },
            // recv consumes the message from the channel, so we can't just use
            // a regular return, because then next_request wouldn't see it
            //
            // a better approach would be to do select on top-level entry point,
            // then we'd be sure we're able to cancel when anything happens, not
            // just when waiting for pending_responses.
            Some(RuntimeRequest::Shutdown) = self.requests.recv_signal() => {
                ControlFlow::Break(())
            }
            else => ControlFlow::Break(())
        }
    }

    pub fn send_response_once_done(&mut self, pending_result: PendingResult) {
        self.pending_responses.push(pending_result);
    }
}
