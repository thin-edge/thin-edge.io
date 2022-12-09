use crate::c8y_http_proxy::messages::{C8YRestRequest, C8YRestResponse};
use async_trait::async_trait;
use tedge_actors::{new_mailbox, Actor, Address, ChannelError, Mailbox, DynSender};
use tedge_http_ext::{HttpActorInstance, HttpRequest, HttpResult};

struct C8YHttpProxyActor {}

#[async_trait]
impl Actor for C8YHttpProxyActor {
    type Input = C8YRestRequest;
    type Output = C8YRestResponse;
    type Mailbox = Mailbox<C8YRestRequest>;
    type Peers = DynSender<Self::Output>;

    async fn run(self, messages: Self::Mailbox, peers: Self::Peers) -> Result<(), ChannelError> {
        todo!()
    }
}

struct C8YHttpProxyPeers {
    /// Requests received by this actor from its clients
    requests: Mailbox<C8YRestRequest>,

    /// Responses sent by this actor to its clients
    responses: DynSender<C8YRestResponse>,

    /// Requests sent by this actor over HTTP
    http_requests: DynSender<HttpRequest>,

    /// Responses received by this actor over HTTP
    http_responses: Mailbox<HttpResult>,
}

impl C8YHttpProxyPeers {
    pub async fn send_http_request(&mut self, request: HttpRequest) -> Result<HttpResult, ChannelError> {
        self.http_requests.send(request).await?;
        self.http_responses.next().await.ok_or(ChannelError::ReceiveError())
    }
}

impl C8YHttpProxyActor {
    pub async fn run(self, mut peers: C8YHttpProxyPeers) -> Result<(), ChannelError> {
        while let Some(request) = peers.requests.next().await {
            match request {
                C8YRestRequest::C8yCreateEvent(_) => {



                }
                C8YRestRequest::C8yUpdateSoftwareListResponse(_) => {}
                C8YRestRequest::UploadLogBinary(_) => {}
                C8YRestRequest::UploadConfigFile(_) => {}
            }
        }
        Ok(())
    }
}
