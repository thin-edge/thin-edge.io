use crate::actor::C8YHttpProxyActor;
use crate::actor::C8YHttpProxyMessageBox;
use crate::credentials::HttpHeaderResult;
use crate::credentials::HttpHeaderRetriever;
use crate::messages::C8YRestRequest;
use crate::messages::C8YRestResult;
use std::convert::Infallible;
use std::path::PathBuf;
use std::time::Duration;
use tedge_actors::Builder;
use tedge_actors::ClientMessageBox;
use tedge_actors::DynSender;
use tedge_actors::MessageSink;
use tedge_actors::RequestEnvelope;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::ServerMessageBoxBuilder;
use tedge_actors::Service;
use tedge_config::ConfigNotSet;
use tedge_config::MultiError;
use tedge_config::ReadError;
use tedge_config::TEdgeConfig;
use tedge_http_ext::HttpRequest;
use tedge_http_ext::HttpResult;

mod actor;
pub mod credentials;
pub mod handle;
pub mod messages;

use c8y_api::proxy_url::Protocol;
use c8y_api::proxy_url::ProxyUrlGenerator;
pub use http::HeaderMap;

#[cfg(test)]
mod tests;

/// Configuration of C8Y REST API
#[derive(Clone)]
pub struct C8YHttpConfig {
    pub c8y_http_host: String,
    pub c8y_mqtt_host: String,
    pub device_id: String,
    pub tmp_dir: PathBuf,
    retry_interval: Duration,
    proxy: ProxyUrlGenerator,
}

impl C8YHttpConfig {
    pub fn try_new(
        tedge_config: &TEdgeConfig,
        c8y_profile: Option<&str>,
    ) -> Result<Self, C8yHttpConfigBuildError> {
        let c8y_http_host = tedge_config
            .c8y
            .try_get(c8y_profile)?
            .http
            .or_config_not_set()?
            .to_string();
        let c8y_mqtt_host = tedge_config
            .c8y
            .try_get(c8y_profile)?
            .mqtt
            .or_config_not_set()?
            .to_string();
        let device_id = tedge_config.device.id.try_read(tedge_config)?.to_string();
        let tmp_dir = tedge_config.tmp.path.as_std_path().to_path_buf();
        let retry_interval = Duration::from_secs(5);

        // Temporary code: this will be deprecated along c8y_http_proxy
        let c8y_config = tedge_config.c8y.try_get(c8y_profile)?;
        let auth_proxy_addr = c8y_config.proxy.client.host.clone();
        let auth_proxy_port = c8y_config.proxy.client.port;
        let auth_proxy_protocol = c8y_config
            .proxy
            .cert_path
            .or_none()
            .map_or(Protocol::Http, |_| Protocol::Https);
        let proxy = ProxyUrlGenerator::new(auth_proxy_addr, auth_proxy_port, auth_proxy_protocol);

        Ok(Self {
            c8y_http_host,
            c8y_mqtt_host,
            proxy,
            device_id,
            tmp_dir,
            retry_interval,
        })
    }
}

/// The errors that could occur while building `C8YHttpConfig` struct.
#[derive(Debug, thiserror::Error)]
pub enum C8yHttpConfigBuildError {
    #[error(transparent)]
    FromReadError(#[from] ReadError),

    #[error(transparent)]
    FromConfigNotSet(#[from] ConfigNotSet),

    #[error(transparent)]
    FromMultiError(#[from] MultiError),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// A proxy to C8Y REST API
pub struct C8YHttpProxyBuilder {
    /// Config
    config: C8YHttpConfig,

    /// Message box for client requests and responses
    clients: ServerMessageBoxBuilder<C8YRestRequest, C8YRestResult>,

    /// Connection to an HTTP actor
    http: ClientMessageBox<HttpRequest, HttpResult>,

    /// Connection to an HTTP header value retriever
    header_retriever: HttpHeaderRetriever,
}

impl C8YHttpProxyBuilder {
    pub fn new(
        config: C8YHttpConfig,
        http: &mut impl Service<HttpRequest, HttpResult>,
        header_retriever: &mut impl Service<(), HttpHeaderResult>,
    ) -> Self {
        let clients = ServerMessageBoxBuilder::new("C8Y-REST", 10);
        let http = ClientMessageBox::new(http);
        let header_retriever = HttpHeaderRetriever::new(header_retriever);
        C8YHttpProxyBuilder {
            config,
            clients,
            http,
            header_retriever,
        }
    }
}

impl Builder<C8YHttpProxyActor> for C8YHttpProxyBuilder {
    type Error = Infallible;

    fn try_build(self) -> Result<C8YHttpProxyActor, Self::Error> {
        Ok(self.build())
    }

    fn build(self) -> C8YHttpProxyActor {
        let message_box = C8YHttpProxyMessageBox {
            clients: self.clients.build(),
            http: self.http,
            header_retriever: self.header_retriever,
        };

        C8YHttpProxyActor::new(self.config, message_box)
    }
}

impl MessageSink<RequestEnvelope<C8YRestRequest, C8YRestResult>> for C8YHttpProxyBuilder {
    fn get_sender(&self) -> DynSender<RequestEnvelope<C8YRestRequest, C8YRestResult>> {
        self.clients.get_sender()
    }
}

impl RuntimeRequestSink for C8YHttpProxyBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.clients.get_signal_sender()
    }
}
