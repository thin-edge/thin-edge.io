use crate::Actor;
use crate::ConcurrentServerMessageBox;
use crate::MessageReceiver;
use crate::RequestEnvelope;
use crate::RuntimeError;
use crate::RuntimeRequest;
use crate::Sender;
use crate::Server;
use crate::ServerMessageBox;
use async_trait::async_trait;

/// An actor that wraps a request-response server
///
/// Requests are processed in turn, leading either to a response or an error.
pub struct ServerActor<S: Server> {
    server: S,
    requests: ServerMessageBox<S::Request, S::Response>,
}

impl<S: Server> ServerActor<S> {
    pub fn new(server: S, requests: ServerMessageBox<S::Request, S::Response>) -> Self {
        ServerActor { server, requests }
    }
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
                    // Ignore errors on send: the requester is simply no more expecting a response
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
            let request_handler = tokio::spawn(async move {
                let result = server.handle(request).await;
                // Ignore errors on send: the requester is simply no more expecting a response
                let _ = reply_to.send(result).await;
            });

            // Send the response back to the client
            self.messages.register_request_handler(request_handler)
        }

        Ok(())
    }
}
