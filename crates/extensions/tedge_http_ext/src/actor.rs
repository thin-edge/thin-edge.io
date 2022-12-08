use crate::{HttpConfig, HttpError, HttpRequest, HttpResponse, HttpResult};
use async_trait::async_trait;
use tedge_actors::{Actor, ChannelError, Mailbox, Recipient};

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
        requests: Self::Mailbox,
        responses: Self::Peers,
    ) -> Result<(), ChannelError> {
        let mut peers = HttpPeers::new(4, requests, responses);

        while let Some((client_id, request)) = peers.next_request().await {
            let request = request.into();
            let client = self.client.clone();

            // Spawn the request
            let pending_result = tokio::spawn(async move {
                let response = match client.execute(request).await {
                    Ok(res) => Ok(res.into()),
                    Err(err) => Err(err.into()),
                };
                (client_id, response)
            });

            // Send the response back to the client
            peers.send_response_once_done(pending_result);
        }

        Ok(())
    }
}

type PendingResult = tokio::task::JoinHandle<(usize, HttpResult)>;

struct HttpPeers {
    /// Max concurrent requests
    max_concurrency: usize,

    /// Requests received by this actor from its clients
    requests: Mailbox<(usize, HttpRequest)>,

    /// Responses sent by this actor to its clients
    responses: Recipient<(usize, HttpResult)>,

    /// Pending responses
    pending_responses: futures::stream::FuturesUnordered<PendingResult>,
}

use futures::StreamExt;

impl HttpPeers {
    fn new(
        max_concurrency: usize,
        requests: Mailbox<(usize, HttpRequest)>,
        responses: Recipient<(usize, HttpResult)>,
    ) -> HttpPeers {
        HttpPeers {
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
