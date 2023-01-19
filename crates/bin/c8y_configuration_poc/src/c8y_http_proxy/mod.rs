use crate::c8y_http_proxy::actor::C8YHttpProxyActor;
use crate::c8y_http_proxy::actor::C8YHttpProxyMessageBox;
use crate::c8y_http_proxy::credentials::JwtResult;
use crate::c8y_http_proxy::credentials::JwtRetriever;
use crate::c8y_http_proxy::handle::C8YHttpHandleBuilder;
use crate::c8y_http_proxy::handle::C8YHttpProxy;
use crate::c8y_http_proxy::messages::C8YRestRequest;
use crate::c8y_http_proxy::messages::C8YRestResult;
use async_trait::async_trait;
use tedge_actors::ActorBuilder;
use tedge_actors::Builder;
use tedge_actors::MessageBoxConnector;
use tedge_actors::MessageBoxPort;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeHandle;
use tedge_actors::ServiceMessageBoxBuilder;
use tedge_config::C8yUrlSetting;
use tedge_config::ConfigSettingAccessor;
use tedge_config::DeviceIdSetting;
use tedge_config::TEdgeConfig;
use tedge_config::TEdgeConfigError;
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

impl TryFrom<TEdgeConfig> for C8YHttpConfig {
    type Error = TEdgeConfigError;

    fn try_from(tedge_config: TEdgeConfig) -> Result<Self, Self::Error> {
        let c8y_host = tedge_config.query(C8yUrlSetting)?;
        let device_id = tedge_config.query(DeviceIdSetting)?;
        Ok(Self {
            c8y_host: c8y_host.into(),
            device_id,
        })
    }
}

pub trait C8YConnectionBuilder: MessageBoxConnector<C8YRestRequest, C8YRestResult, ()> {
    fn new_c8y_handle(&mut self, client_name: &str) -> C8YHttpProxy;
}
impl C8YConnectionBuilder for C8YHttpProxyBuilder {
    fn new_c8y_handle(&mut self, client_name: &str) -> C8YHttpProxy {
        let mut port = C8YHttpHandleBuilder::new(client_name);
        self.connect(&mut port);
        port.build()
    }
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
        jwt: &mut impl MessageBoxConnector<(), JwtResult, ()>,
    ) -> Self {
        let clients = ServiceMessageBoxBuilder::new("C8Y-REST", 10);
        let http = HttpHandle::new("C8Y-REST => HTTP", http, ());
        let jwt = JwtRetriever::new("C8Y-REST => JWT", jwt, ());
        C8YHttpProxyBuilder {
            config,
            clients,
            http,
            jwt,
        }
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

impl MessageBoxConnector<C8YRestRequest, C8YRestResult, ()> for C8YHttpProxyBuilder {
    fn connect_with(
        &mut self,
        peer: &mut impl MessageBoxPort<C8YRestRequest, C8YRestResult>,
        config: (),
    ) {
        self.clients.connect_with(peer, config)
    }
}
