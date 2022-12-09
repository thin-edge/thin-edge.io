use crate::c8y_http_proxy::messages::{C8YRestRequest, C8YRestResponse};
use async_trait::async_trait;
use tedge_actors::{
    new_mailbox, ActorBuilder, Address, DynSender, LinkError, Mailbox, PeerLinker, RuntimeError,
    RuntimeHandle,
};
use tedge_http_ext::{HttpRequest, HttpResult};

mod actor;
mod messages;

/// Configuration of C8Y REST API
pub struct C8YHttpConfig {
    pub c8y_url: String,
}

/// A proxy to C8Y REST API
///
/// This is an actor builder.
pub struct C8YHttpProxyBuilder {
    /// Config
    config: C8YHttpConfig,

    /// Mailbox & address for peers requests
    requests: (Mailbox<C8YRestRequest>, Address<C8YRestRequest>),

    /// Mailbox & address for HTTP responses
    http_responses: (Mailbox<HttpResult>, Address<HttpResult>),

    /// To be connected to some clients
    ///
    /// If None is given, there is no point to spawn this actor
    responses: Option<DynSender<C8YRestResponse>>,

    /// To be connected to the HTTP actor
    ///
    /// If None is given, this actor cannot run
    http_requests: Option<DynSender<HttpRequest>>,
}

impl C8YHttpProxyBuilder {
    fn new(config: C8YHttpConfig) -> Self {
        C8YHttpProxyBuilder {
            config,
            requests: new_mailbox(10),
            http_responses: new_mailbox(1),
            responses: None,
            http_requests: None,
        }
    }

    /// Connect this instance to some http connection provider
    pub fn set_http_connection(
        &mut self,
        http: &mut impl PeerLinker<HttpRequest, HttpResult>,
    ) -> Result<(), LinkError> {
        let http_requests = http.connect(self.http_responses.1.clone().into())?;
        self.http_requests = Some(http_requests);
        Ok(())
    }
}

#[async_trait]
impl ActorBuilder for C8YHttpProxyBuilder {
    async fn spawn(self, runtime: &mut RuntimeHandle) -> Result<(), RuntimeError> {
        todo!()
    }
}

impl PeerLinker<C8YRestRequest, C8YRestResponse> for C8YHttpProxyBuilder {
    fn connect(
        &mut self,
        output_sender: DynSender<C8YRestResponse>,
    ) -> Result<DynSender<C8YRestRequest>, LinkError> {
        if self.responses.is_some() {
            return Err(LinkError::ExcessPeer {
                role: "input requests".into(),
            });
        }

        self.responses = Some(output_sender);
        Ok(self.requests.1.clone().into())
    }
}
