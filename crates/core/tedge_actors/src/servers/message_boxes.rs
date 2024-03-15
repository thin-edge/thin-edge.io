use crate::ChannelError;
use crate::CloneSender;
use crate::DynSender;
use crate::LoggingReceiver;
use crate::Message;
use crate::MessageReceiver;
use crate::MessageSink;
use crate::RequestEnvelope;
use crate::RuntimeRequest;
use crate::Sender;
use async_trait::async_trait;
use futures::channel::oneshot;
use futures::StreamExt;
use std::fmt::Debug;
use std::ops::ControlFlow;

/// A message box for a request-response server
pub type ServerMessageBox<Request, Response> = LoggingReceiver<RequestEnvelope<Request, Response>>;

/// A message box for services that handles requests concurrently
pub struct ConcurrentServerMessageBox<Request: Debug, Response> {
    /// Max concurrent requests
    max_concurrency: usize,

    /// Message box to interact with clients of this service
    requests: ServerMessageBox<Request, Response>,

    /// Pending responses
    running_request_handlers: futures::stream::FuturesUnordered<RequestHandler>,
}

type RequestHandler = tokio::task::JoinHandle<()>;

impl<Request: Message, Response: Message> ConcurrentServerMessageBox<Request, Response> {
    pub(crate) fn new(
        max_concurrency: usize,
        requests: ServerMessageBox<Request, Response>,
    ) -> Self {
        ConcurrentServerMessageBox {
            max_concurrency,
            requests,
            running_request_handlers: futures::stream::FuturesUnordered::new(),
        }
    }

    pub async fn next_request(&mut self) -> Option<RequestEnvelope<Request, Response>> {
        if self.await_idle_processor().await.is_break() {
            return None;
        }

        loop {
            tokio::select! {
                Some(request) = self.requests.recv() => {
                    return Some(request);
                }
                Some(result) = self.running_request_handlers.next() => {
                    if let Err(err) = result {
                        log::error!("Fail to run a request to completion: {err}");
                    }
                }
                else => {
                    return None
                }
            }
        }
    }

    async fn await_idle_processor(&mut self) -> ControlFlow<(), ()> {
        if self.running_request_handlers.len() < self.max_concurrency {
            return ControlFlow::Continue(());
        }

        tokio::select! {
            Some(result) = self.running_request_handlers.next() => {
                if let Err(err) = result {
                    log::error!("Fail to run a request to completion: {err}");
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

    pub fn register_request_handler(&mut self, pending_result: RequestHandler) {
        self.running_request_handlers.push(pending_result);
    }
}

/// A message box used by a client to request a server and await the responses.
pub struct ClientMessageBox<Request, Response> {
    sender: DynSender<RequestEnvelope<Request, Response>>,
}

impl<Request: Message, Response: Message> Clone for ClientMessageBox<Request, Response> {
    fn clone(&self) -> Self {
        ClientMessageBox {
            sender: self.sender.sender_clone(),
        }
    }
}

impl<Request: Message, Response: Message> ClientMessageBox<Request, Response> {
    /// Create a [ClientMessageBox] connected to a given [Server](crate::Server)
    pub fn new(server: &mut impl MessageSink<RequestEnvelope<Request, Response>>) -> Self {
        ClientMessageBox {
            sender: server.get_sender(),
        }
    }

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
pub(crate) struct RequestSender<Request: 'static, Response: 'static> {
    pub(crate) sender: DynSender<RequestEnvelope<Request, Response>>,
    pub(crate) reply_to: DynSender<Response>,
}

impl<Request, Response> Clone for RequestSender<Request, Response> {
    fn clone(&self) -> Self {
        RequestSender {
            sender: self.sender.sender_clone(),
            reply_to: self.reply_to.sender_clone(),
        }
    }
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

#[cfg(test)]
#[cfg(feature = "test-helpers")]
mod tests {
    use super::*;

    use crate::test_helpers::ServiceProviderExt;
    use crate::Builder;
    use crate::ConcurrentServerActor;
    use crate::DynSender;
    use crate::Runtime;
    use crate::RuntimeRequest;
    use crate::RuntimeRequestSink;
    use crate::Server;
    use crate::ServerMessageBoxBuilder;
    use crate::SimpleMessageBox;
    use async_trait::async_trait;
    use std::time::Duration;
    use tokio::sync::mpsc::error::TryRecvError;
    use tokio::time::timeout;

    #[tokio::test]
    async fn only_processes_messages_up_to_max_concurrency() {
        let mut builder = ServerMessageBoxBuilder::new("ConcurrentServerMessageBoxTest", 16);
        let mut test_box = builder.new_client_box();
        let message_box: ServerMessageBox<i32, i32> = builder.build();
        let mut concurrent_box = ConcurrentServerMessageBox::new(4, message_box);

        // to pause initial 4 tasks
        let (resume_tx, resume_rx) = tokio::sync::oneshot::channel::<()>();

        // use other channel to return results from tasks because it has
        // `try_recv` which doesn't block
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        // send all messages to the concurrent message box
        for i in 0..5 {
            test_box.send(i).await.unwrap();
        }

        // spawn 1st request that we're going to pause/resume
        tokio::spawn(async move {
            let request = concurrent_box.next_request().await.unwrap();
            concurrent_box.register_request_handler(tokio::spawn(async move {
                resume_rx.await.unwrap();
            }));
            // After a call to `send_response_once_done` finishes, we
            // consider the task to have started executing
            tx.send(request).unwrap();

            loop {
                let request = concurrent_box.next_request().await.unwrap();
                concurrent_box.register_request_handler(tokio::spawn(async move {
                    // keep other requests executing
                    std::future::pending::<()>().await
                }));
                tx.send(request).unwrap();
            }
        });

        // Expect first 4 tasks to be in-progress
        assert_eq!(rx.recv().await.map(|r| r.request), Some(0));
        assert_eq!(rx.recv().await.map(|r| r.request), Some(1));
        assert_eq!(rx.recv().await.map(|r| r.request), Some(2));
        assert_eq!(rx.recv().await.map(|r| r.request), Some(3));

        // Expect at this point in time that 5th task hasn't started executing
        // yet
        assert_eq!(rx.try_recv().unwrap_err(), TryRecvError::Empty);

        // finish 1st task
        resume_tx.send(()).unwrap();

        // expect 5th task started executing only after 1st completed
        assert_eq!(rx.recv().await.map(|r| r.request), Some(4));
    }

    // The purpose of the test is to check that the server which uses a
    // ConcurrentServerMessageBox terminates in a reasonable timeframe after
    // receiving a shutdown request from the runtime. For this purpose we create
    // a server which never completes its requests, and after filling it with
    // requests up to its max concurrency level, we terminate the runtime.
    #[tokio::test]
    async fn does_not_block_runtime_exit() {
        #[derive(Clone)]
        struct TestServer {
            test_tx: tokio::sync::mpsc::UnboundedSender<i32>,
        }

        #[async_trait]
        impl Server for TestServer {
            type Request = i32;
            type Response = i32;

            fn name(&self) -> &str {
                ""
            }

            async fn handle(&mut self, request: Self::Request) -> Self::Response {
                // let the caller know processing request started but don't
                // complete it
                self.test_tx.send(request).unwrap();
                std::future::pending().await
            }
        }

        struct TestServerBuilder {
            box_builder: ServerMessageBoxBuilder<i32, i32>,
            test_tx: tokio::sync::mpsc::UnboundedSender<i32>,
        }

        impl TestServerBuilder {
            fn new() -> (Self, tokio::sync::mpsc::UnboundedReceiver<i32>) {
                let box_builder =
                    ServerMessageBoxBuilder::new("ConcurrentServerMessageBoxTest", 16);

                let (test_tx, test_rx) = tokio::sync::mpsc::unbounded_channel();

                (
                    Self {
                        box_builder,
                        test_tx,
                    },
                    test_rx,
                )
            }

            fn client_box(&mut self) -> SimpleMessageBox<i32, i32> {
                self.box_builder.new_client_box()
            }
        }

        impl Builder<ConcurrentServerActor<TestServer>> for TestServerBuilder {
            type Error = std::convert::Infallible;

            fn try_build(self) -> Result<ConcurrentServerActor<TestServer>, Self::Error> {
                let message_box: ServerMessageBox<i32, i32> = self.box_builder.build();
                let message_box = ConcurrentServerMessageBox::new(4, message_box);
                Ok(ConcurrentServerActor::new(
                    TestServer {
                        test_tx: self.test_tx,
                    },
                    message_box,
                ))
            }
        }

        impl RuntimeRequestSink for TestServerBuilder {
            fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
                self.box_builder.get_signal_sender()
            }
        }

        let mut runtime = Runtime::new();

        let (mut server_actor_builder, mut test_rx) = TestServerBuilder::new();
        let mut client_box = server_actor_builder.client_box();

        runtime.spawn(server_actor_builder).await.unwrap();

        let mut handle = runtime.get_handle();
        tokio::spawn(async move {
            client_box.send(1).await.unwrap();
            client_box.send(2).await.unwrap();
            client_box.send(3).await.unwrap();
            client_box.send(4).await.unwrap();
            client_box.send(5).await.unwrap();

            // ensure the server processes all requests under its
            // max_concurrency requirements
            assert_eq!(test_rx.recv().await, Some(1));
            assert_eq!(test_rx.recv().await, Some(2));
            assert_eq!(test_rx.recv().await, Some(3));
            assert_eq!(test_rx.recv().await, Some(4));

            handle.shutdown().await.unwrap()
        });

        assert_eq!(
            timeout(Duration::from_millis(500), async {
                runtime.run_to_completion().await.unwrap()
            })
            .await,
            Ok(())
        );
    }
}
