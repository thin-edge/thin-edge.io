use crate::c8y_http_proxy::handle::C8YHttpProxy;
use crate::c8y_http_proxy::messages::C8YRestRequest;
use crate::c8y_http_proxy::messages::C8YRestResponse;
use async_trait::async_trait;
use tedge_actors::mpsc;
use tedge_actors::ActorBuilder;
use tedge_actors::DynSender;
use tedge_actors::LinkError;
use tedge_actors::PeerLinker;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeHandle;
use tedge_http_ext::HttpRequest;
use tedge_http_ext::HttpResult;

mod actor;
pub mod handle;
pub mod messages;

/// Configuration of C8Y REST API
#[derive(Default)]
pub struct C8YHttpConfig {
    pub c8y_url: String,
}

/// A proxy to C8Y REST API
///
/// This is an actor builder.
pub struct C8YHttpProxyBuilder {
    /// Config
    _config: C8YHttpConfig,

    /// Sender and receiver for peers requests
    requests: (mpsc::Sender<C8YRestRequest>, mpsc::Receiver<C8YRestRequest>),

    /// Sender and receiver for HTTP responses
    http_responses: (mpsc::Sender<HttpResult>, mpsc::Receiver<HttpResult>),

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
    pub fn new(config: C8YHttpConfig) -> Self {
        C8YHttpProxyBuilder {
            _config: config,
            requests: mpsc::channel(10),
            http_responses: mpsc::channel(1),
            responses: None,
            http_requests: None,
        }
    }

    /// Connect this instance to some http connection provider
    pub fn with_http_connection(
        &mut self,
        http: &mut impl PeerLinker<HttpRequest, HttpResult>,
    ) -> Result<(), LinkError> {
        let http_requests = http.connect(self.http_responses.0.clone().into())?;
        self.http_requests = Some(http_requests);
        Ok(())
    }

    /// Return a new handle to the actor under construction
    pub fn handle(&mut self) -> C8YHttpProxy {
        C8YHttpProxy::new(self)
    }
}

#[async_trait]
impl ActorBuilder for C8YHttpProxyBuilder {
    async fn spawn(self, _runtime: &mut RuntimeHandle) -> Result<(), RuntimeError> {
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
        Ok(self.requests.0.clone().into())
    }
}
