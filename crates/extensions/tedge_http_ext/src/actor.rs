use crate::HttpConfig;
use crate::HttpError;
use crate::HttpRequest;
use crate::HttpResult;
use async_trait::async_trait;
use futures::channel::mpsc;
use futures::StreamExt;
use tedge_actors::Actor;
use tedge_actors::ChannelError;
use tedge_actors::DynSender;
use tedge_actors::MessageBox;

pub(crate) struct HttpActor {
    client: hyper::client::Client<hyper::client::connect::HttpConnector, hyper::body::Body>,
}

impl HttpActor {
    pub(crate) fn new(_config: HttpConfig) -> Result<Self, HttpError> {
        let client = hyper::client::Client::builder().build_http();
        Ok(HttpActor { client })
    }
}

#[async_trait]
impl Actor for HttpActor {
    type MessageBox = HttpMessageBox;

    fn name(&self) -> &str {
        "HTTP"
    }

    async fn run(self, mut messages: HttpMessageBox) -> Result<(), ChannelError> {
        while let Some((client_id, request)) = messages.recv().await {
            let client = self.client.clone();

            // Spawn the request
            let pending_result = tokio::spawn(async move {
                let response = client.request(request).await;
                (client_id, response)
            });

            // Send the response back to the client
            messages.send_response_once_done(pending_result)
        }

        Ok(())
    }
}

type PendingResult = tokio::task::JoinHandle<(usize, HttpResult)>;

pub(crate) struct HttpMessageBox {
    /// Max concurrent requests
    max_concurrency: usize,

    /// Requests received by this actor from its clients
    requests: mpsc::Receiver<(usize, HttpRequest)>,

    /// Responses sent by this actor to its clients
    responses: DynSender<(usize, HttpResult)>,

    /// Pending responses
    pending_responses: futures::stream::FuturesUnordered<PendingResult>,
}

impl HttpMessageBox {
    pub(crate) fn new(
        max_concurrency: usize,
        requests: mpsc::Receiver<(usize, HttpRequest)>,
        responses: DynSender<(usize, HttpResult)>,
    ) -> HttpMessageBox {
        HttpMessageBox {
            max_concurrency,
            requests,
            responses,
            pending_responses: futures::stream::FuturesUnordered::new(),
        }
    }

    async fn next_request(&mut self) -> Option<(usize, HttpRequest)> {
        self.await_idle_processor().await;
        loop {
            tokio::select! {
                Some(request) = self.requests.next() => {
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

    async fn await_idle_processor(&mut self) {
        if self.pending_responses.len() >= self.max_concurrency {
            if let Some(result) = self.pending_responses.next().await {
                self.send_result(result).await;
            }
        }
    }

    fn send_response_once_done(&mut self, pending_result: PendingResult) {
        self.pending_responses.push(pending_result);
    }

    async fn send_result(&mut self, result: Result<(usize, HttpResult), tokio::task::JoinError>) {
        if let Ok(response) = result {
            let _ = self.responses.send(response).await;
        }
        // TODO handle error cases:
        // - cancelled task
        // - task panics
        // - send fails
    }
}

#[async_trait]
impl MessageBox for HttpMessageBox {
    type Input = (usize, HttpRequest);
    type Output = (usize, HttpResult);

    async fn recv(&mut self) -> Option<Self::Input> {
        self.next_request().await
    }

    async fn send(&mut self, message: Self::Output) -> Result<(), ChannelError> {
        self.responses.send(message).await
    }

    fn new_box(
        _name: &str,
        capacity: usize,
        output: DynSender<Self::Output>,
    ) -> (DynSender<Self::Input>, Self) {
        let max_concurrency = 4;
        let (request_sender, request_receiver) = mpsc::channel(capacity);
        let message_box = HttpMessageBox::new(max_concurrency, request_receiver, output);
        (request_sender.into(), message_box)
    }

    fn turn_logging_on(&mut self, _on: bool) {}

    fn name(&self) -> &str {
        "HTTP"
    }

    fn logging_is_on(&self) -> bool {
        true
    }
}
