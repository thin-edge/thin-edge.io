use crate::Builder;
use crate::ChannelError;
use crate::Message;
use crate::MessageReceiver;
use crate::NoConfig;
use crate::RuntimeRequest;
use crate::Sender;
use crate::ServiceConsumer;
use crate::ServiceProvider;
use crate::SimpleMessageBox;
use crate::SimpleMessageBoxBuilder;
use futures::StreamExt;
use std::fmt::Debug;
use std::ops::ControlFlow;
use tokio::select;

/// A message box for a request-response server
pub type ServerMessageBox<Request, Response> =
    SimpleMessageBox<(ClientId, Request), (ClientId, Response)>;

/// Internal id assigned to a client actor of a server actor
pub type ClientId = usize;

/// A message box for services that handles requests concurrently
pub struct ConcurrentServerMessageBox<Request: Debug, Response> {
    /// Max concurrent requests
    max_concurrency: usize,

    /// Message box to interact with clients of this service
    clients: ServerMessageBox<Request, Response>,

    /// Pending responses
    pending_responses: futures::stream::FuturesUnordered<PendingResult<(usize, Response)>>,
}

type PendingResult<R> = tokio::task::JoinHandle<R>;

impl<Request: Message, Response: Message> ConcurrentServerMessageBox<Request, Response> {
    pub(crate) fn new(
        max_concurrency: usize,
        clients: ServerMessageBox<Request, Response>,
    ) -> Self {
        ConcurrentServerMessageBox {
            max_concurrency,
            clients,
            pending_responses: futures::stream::FuturesUnordered::new(),
        }
    }

    pub async fn recv(&mut self) -> Option<(ClientId, Request)> {
        self.next_request().await
    }

    pub async fn send(&mut self, message: (ClientId, Response)) -> Result<(), ChannelError> {
        self.clients.send(message).await
    }

    async fn next_request(&mut self) -> Option<(usize, Request)> {
        if self.await_idle_processor().await.is_break() {
            return None;
        }

        loop {
            tokio::select! {
                Some(request) = self.clients.recv() => {
                    return Some(request);
                }
                Some(result) = self.pending_responses.next() => {
                    self.send_result(result).await;
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

        select! {
            Some(result) = self.pending_responses.next() => {
                self.send_result(result).await;
                ControlFlow::Continue(())
            },
            // recv consumes the message from the channel, so we can't just use
            // a regular return, because then next_request wouldn't see it
            //
            // a better approach would be to do select on top-level entry point,
            // then we'd be sure we're able to cancel when anything happens, not
            // just when waiting for pending_responses, e.g. if send_result
            // stalls
            Some(RuntimeRequest::Shutdown) = self.clients.recv_signal() => {
                ControlFlow::Break(())
            }
            else => ControlFlow::Break(())
        }
    }

    pub fn send_response_once_done(&mut self, pending_result: PendingResult<(ClientId, Response)>) {
        self.pending_responses.push(pending_result);
    }

    async fn send_result(&mut self, result: Result<(usize, Response), tokio::task::JoinError>) {
        if let Ok(response) = result {
            let _ = self.clients.send(response).await;
        }
        // TODO handle error cases:
        // - cancelled task
        // - task panics
        // - send fails
    }
}

/// Client side handler of requests/responses sent to an actor
/// Client side handler that allows you to send requests to an actor
/// and synchronously wait for its response using the `await_response` function.
///
/// Note that this message box sends requests and receive responses.
pub struct ClientMessageBox<Request, Response: Debug> {
    messages: SimpleMessageBox<Response, Request>,
}

impl<Request: Message, Response: Message> ClientMessageBox<Request, Response> {
    /// Create a new `ClientMessageBox` connected to the service.
    pub fn new(
        client_name: &str,
        service: &mut impl ServiceProvider<Request, Response, NoConfig>,
    ) -> Self {
        let capacity = 1; // At most one response is ever expected
        let messages = SimpleMessageBoxBuilder::new(client_name, capacity)
            .with_connection(service)
            .build();
        ClientMessageBox { messages }
    }

    /// Send the request and await for a response
    pub async fn await_response(&mut self, request: Request) -> Result<Response, ChannelError> {
        self.messages.send(request).await?;
        self.messages
            .recv()
            .await
            .ok_or(ChannelError::ReceiveError())
    }
}

#[cfg(test)]
#[cfg(feature = "test-helpers")]
mod tests {
    use super::*;

    use crate::test_helpers::MessageReceiverExt;
    use crate::test_helpers::ServiceProviderExt;
    use crate::ConcurrentServerActor;
    use crate::DynSender;
    use crate::Runtime;
    use crate::RuntimeRequest;
    use crate::RuntimeRequestSink;
    use crate::Server;
    use crate::ServerMessageBoxBuilder;
    use async_trait::async_trait;
    use std::time::Duration;
    use tokio::sync::mpsc::error::TryRecvError;
    use tokio::time::timeout;

    #[tokio::test]
    async fn only_processes_messages_up_to_max_concurrency() {
        let mut builder = SimpleMessageBoxBuilder::new("ConcurrentServerMessageBoxTest", 16);
        let mut test_box = builder.new_client_box(NoConfig);
        let message_box: ServerMessageBox<i32, i32> = builder.build();
        let mut concurrent_box = ConcurrentServerMessageBox::new(4, message_box);

        // to pause initial 4 tasks
        let (resume_tx, resume_rx) = tokio::sync::oneshot::channel::<()>();

        // use other channel to return results from tasks because it has
        // `try_recv` which doesn't block
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        // send all messages to the concurrent message box
        for i in 0..5 {
            test_box.send((i as usize, i)).await.unwrap();
        }

        // spawn 1st request that we're going to pause/resume
        tokio::spawn(async move {
            let request = concurrent_box.recv().await.unwrap();
            concurrent_box.send_response_once_done(tokio::spawn(async move {
                resume_rx.await.unwrap();
                request
            }));
            // After a call to `send_response_once_done` finishes, we
            // consider the task to have started executing
            tx.send(request).unwrap();

            loop {
                let request = concurrent_box.recv().await.unwrap();
                concurrent_box.send_response_once_done(tokio::spawn(async move {
                    // keep other requests executing
                    std::future::pending::<()>().await;
                    request
                }));
                tx.send(request).unwrap();
            }
        });

        // Expect first 4 tasks to be in-progress
        assert_eq!(rx.recv().await, Some((0usize, 0)));
        assert_eq!(rx.recv().await, Some((1usize, 1)));
        assert_eq!(rx.recv().await, Some((2usize, 2)));
        assert_eq!(rx.recv().await, Some((3usize, 3)));

        // Expect at this point in time that 5th task hasn't started executing
        // yet
        assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));

        // finish 1st task
        resume_tx.send(()).unwrap();

        // expect 5th task started executing only after 1st completed
        test_box.assert_received([(0usize, 0)]).await;
        assert_eq!(rx.recv().await, Some((4usize, 4)));
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
                self.box_builder.new_client_box(NoConfig)
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

        let mut runtime = Runtime::try_new(None).await.unwrap();

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
