use crate::actor::C8YHttpProxyMessageBox;
use crate::credentials::JwtResult;
use crate::credentials::JwtRetriever;
use crate::messages::C8YRestRequest;
use crate::messages::C8YRestResult;
use std::convert::Infallible;
use std::path::PathBuf;
use tedge_actors::Builder;
use tedge_actors::ClientMessageBox;
use tedge_actors::DynSender;
use tedge_actors::NoConfig;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::ServerMessageBoxBuilder;
use tedge_actors::ServiceConsumer;
use tedge_actors::ServiceProvider;
use tedge_http_ext::HttpRequest;
use tedge_http_ext::HttpResult;

mod actor;
pub mod credentials;
pub mod handle;
pub mod messages;

#[cfg(test)]
mod tests;

/// Configuration of C8Y REST API
pub struct C8YHttpConfig {
    pub c8y_host: String,
    pub device_id: String,
    pub tmp_dir: PathBuf,
    pub messages: C8YHttpProxyMessageBox,
}

/// A proxy to C8Y REST API
///
/// This is an actor builder.
/// - `impl ServiceProvider<C8YRestRequest, C8YRestResult, NoConfig>`
pub struct C8YHttpProxyBuilder {
    /// Config
    c8y_host: String,
    device_id: String,
    tmp_dir: PathBuf,

    /// Message box for client requests and responses
    clients: ServerMessageBoxBuilder<C8YRestRequest, C8YRestResult>,

    /// Connection to an HTTP actor
    http: ClientMessageBox<HttpRequest, HttpResult>,

    /// Connection to a JWT token retriever
    jwt: JwtRetriever,
}

impl C8YHttpProxyBuilder {
    pub fn new(
        c8y_host: String,
        device_id: String,
        tmp_dir: PathBuf,
        http: &mut impl ServiceProvider<HttpRequest, HttpResult, NoConfig>,
        jwt: &mut impl ServiceProvider<(), JwtResult, NoConfig>,
    ) -> Self {
        let clients = ServerMessageBoxBuilder::new("C8Y-REST", 10);
        let http = ClientMessageBox::new("C8Y-REST => HTTP", http);
        let jwt = JwtRetriever::new("C8Y-REST => JWT", jwt);
        C8YHttpProxyBuilder {
            c8y_host,
            device_id,
            tmp_dir,
            clients,
            http,
            jwt,
        }
    }
}

impl Builder<C8YHttpConfig> for C8YHttpProxyBuilder {
    type Error = Infallible;

    fn try_build(self) -> Result<C8YHttpConfig, Self::Error> {
        Ok(self.build())
    }

    fn build(self) -> C8YHttpConfig {
        let message_box = C8YHttpProxyMessageBox {
            clients: self.clients.build(),
            http: self.http,
            jwt: self.jwt,
        };

        C8YHttpConfig {
            c8y_host: self.c8y_host,
            device_id: self.device_id,
            tmp_dir: self.tmp_dir,
            messages: message_box,
        }
    }
}

impl ServiceProvider<C8YRestRequest, C8YRestResult, NoConfig> for C8YHttpProxyBuilder {
    fn add_peer(
        &mut self,
        peer: &mut impl ServiceConsumer<C8YRestRequest, C8YRestResult, NoConfig>,
    ) {
        self.clients.add_peer(peer)
    }
}

impl RuntimeRequestSink for C8YHttpProxyBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.clients.get_signal_sender()
    }
}

impl Builder<C8YHttpProxyMessageBox> for C8YHttpProxyBuilder {
    type Error = Infallible;

    fn try_build(self) -> Result<C8YHttpProxyMessageBox, Self::Error> {
        Ok(C8YHttpProxyMessageBox {
            clients: self.clients.build(),
            http: self.http,
            jwt: self.jwt,
        })
    }
}
