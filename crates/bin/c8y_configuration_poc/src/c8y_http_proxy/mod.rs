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
use tedge_http_ext::HttpConnectionBuilder;
use tedge_http_ext::HttpHandle;
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

    /// To be connected to some clients
    ///
    /// If None is given, there is no point to spawn this actor
    responses: Option<DynSender<C8YRestResponse>>,

    /// To be connected to the HTTP actor
    ///
    /// If None is given, this actor cannot run
    http: Option<HttpHandle>,
}

impl C8YHttpProxyBuilder {
    pub fn new(config: C8YHttpConfig) -> Self {
        C8YHttpProxyBuilder {
            _config: config,
            requests: mpsc::channel(10),
            responses: None,
            http: None,
        }
    }

    /// Connect this instance to some http connection provider
    pub fn with_http_connection(
        &mut self,
        http: &mut impl HttpConnectionBuilder,
    ) -> Result<(), LinkError> {
        self.http = Some(http.new_handle());
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

pub trait C8YConnectionBuilder {
    fn connect(&mut self, client: DynSender<C8YRestResponse>) -> DynSender<C8YRestRequest>;

    fn new_handle(&mut self) -> C8YHttpProxy {
        C8YHttpProxy::new(self)
    }
}

impl C8YConnectionBuilder for C8YHttpProxyBuilder {
    fn connect(&mut self, output_sender: DynSender<C8YRestResponse>) -> DynSender<C8YRestRequest> {
        self.responses = Some(output_sender);
        self.requests.0.clone().into()
    }
}
