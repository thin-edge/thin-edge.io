use crate::c8y_http_proxy::actor::C8YHttpProxyActor;
use crate::c8y_http_proxy::actor::C8YHttpProxyMessageBox;
use crate::c8y_http_proxy::credentials::JwtResult;
use crate::c8y_http_proxy::credentials::JwtRetriever;
use crate::c8y_http_proxy::handle::C8YHttpProxy;
use crate::c8y_http_proxy::messages::C8YRestRequest;
use crate::c8y_http_proxy::messages::C8YRestResult;
use async_trait::async_trait;
use std::convert::Infallible;
use tedge_actors::ActorBuilder;
use tedge_actors::ConnectionBuilder;
use tedge_actors::DynSender;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeHandle;
use tedge_actors::ServiceMessageBoxBuilder;
use tedge_http_ext::HttpConnectionBuilder;
use tedge_http_ext::HttpHandle;

mod actor;
pub mod credentials;
pub mod handle;
pub mod messages;

/// Configuration of C8Y REST API
#[derive(Default)]
pub struct C8YHttpConfig {
    pub c8y_host: String,
    pub device_id: String,
}

/// A proxy to C8Y REST API
///
/// This is an actor builder.
pub struct C8YHttpProxyBuilder {
    /// Config
    config: C8YHttpConfig,

    /// Message box for client requests and responses
    clients: ServiceMessageBoxBuilder<C8YRestRequest, C8YRestResult>,

    /// Connection to an HTTP actor
    http: HttpHandle,

    /// Connection to a JWT token retriever
    jwt: JwtRetriever,
}

impl C8YHttpProxyBuilder {
    pub fn new(
        config: C8YHttpConfig,
        http: &mut impl HttpConnectionBuilder,
        jwt: &mut impl ConnectionBuilder<(), JwtResult, (), Infallible>,
    ) -> Self {
        let clients = ServiceMessageBoxBuilder::new("C8Y-REST", 10);
        let http = http.new_request_handle(());
        let jwt = jwt.new_request_handle(());
        C8YHttpProxyBuilder {
            config,
            clients,
            http,
            jwt,
        }
    }

    /// Return a new handle to the actor under construction
    pub fn handle(&mut self) -> C8YHttpProxy {
        C8YHttpProxy::new(self)
    }
}

#[async_trait]
impl ActorBuilder for C8YHttpProxyBuilder {
    async fn spawn(self, runtime: &mut RuntimeHandle) -> Result<(), RuntimeError> {
        let actor = C8YHttpProxyActor::new(self.config);
        let message_box = C8YHttpProxyMessageBox {
            clients: self.clients.build(),
            http: self.http,
            jwt: self.jwt,
        };
        runtime.run(actor, message_box).await
    }
}

pub trait C8YConnectionBuilder {
    fn connect(&mut self, client: DynSender<C8YRestResult>) -> DynSender<C8YRestRequest>;

    fn new_handle(&mut self) -> C8YHttpProxy {
        C8YHttpProxy::new(self)
    }
}

impl C8YConnectionBuilder for C8YHttpProxyBuilder {
    fn connect(&mut self, output_sender: DynSender<C8YRestResult>) -> DynSender<C8YRestRequest> {
        self.clients.connect(output_sender)
    }
}
