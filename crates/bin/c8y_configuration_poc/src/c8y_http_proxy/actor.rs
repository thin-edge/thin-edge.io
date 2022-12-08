use crate::c8y_http_proxy::messages::{C8YRestRequest, C8YRestResponse};
use async_trait::async_trait;
use tedge_actors::{new_mailbox, Actor, Address, ChannelError, Mailbox, Recipient};
use tedge_http_ext::{HttpActorInstance, HttpRequest, HttpResult};

struct C8YHttpProxyActor {}

#[async_trait]
impl Actor for C8YHttpProxyActor {
    type Input = C8YRestRequest;
    type Output = C8YRestResponse;
    type Mailbox = Mailbox<C8YRestRequest>;
    type Peers = Recipient<Self::Output>;

    async fn run(self, messages: Self::Mailbox, peers: Self::Peers) -> Result<(), ChannelError> {
        todo!()
    }
}

struct C8YHttpProxyPeers {
    /// Requests received by this actor from its clients
    requests: Address<C8YRestRequest>,

    /// Responses sent by this actor to its clients
    responses: Recipient<C8YRestResponse>,

    /// Requests sent by this actor over HTTP
    http_requests: Recipient<HttpRequest>,

    /// Responses received by this actor over HTTP
    http_responses: Address<HttpResult>,
}
