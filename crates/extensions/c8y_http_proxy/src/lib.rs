use crate::actor::C8YHttpProxyActor;
use crate::actor::C8YHttpProxyMessageBox;
use crate::credentials::JwtResult;
use crate::credentials::JwtRetriever;
use crate::messages::C8YRestRequest;
use crate::messages::C8YRestResult;
use reqwest::Identity;
use std::convert::Infallible;
use std::path::PathBuf;
use std::time::Duration;
use tedge_actors::Builder;
use tedge_actors::ClientMessageBox;
use tedge_actors::DynSender;
use tedge_actors::MessageSink;
use tedge_actors::NoConfig;
use tedge_actors::RequestEnvelope;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::ServerMessageBoxBuilder;
use tedge_actors::Service;
use tedge_config::ConfigNotSet;
use tedge_config::ReadError;
use tedge_config::TEdgeConfig;
use tedge_http_ext::HttpRequest;
use tedge_http_ext::HttpResult;

mod actor;
pub mod credentials;
pub mod handle;
pub mod messages;

#[cfg(test)]
mod tests;

/// Configuration of C8Y REST API
#[derive(Default, Clone)]
pub struct C8YHttpConfig {
    pub c8y_host: String,
    pub device_id: String,
    pub tmp_dir: PathBuf,
    identity: Option<Identity>,
    retry_interval: Duration,
}

impl TryFrom<&TEdgeConfig> for C8YHttpConfig {
    type Error = C8yHttpConfigBuildError;

    fn try_from(tedge_config: &TEdgeConfig) -> Result<Self, Self::Error> {
        let c8y_host = tedge_config.c8y.http.or_config_not_set()?.to_string();
        let device_id = tedge_config.device.id.try_read(tedge_config)?.to_string();
        let tmp_dir = tedge_config.tmp.path.as_std_path().to_path_buf();
        let identity = tedge_config.http.client.auth.identity()?;
        let retry_interval = Duration::from_secs(5);

        Ok(Self {
            c8y_host,
            device_id,
            tmp_dir,
            identity,
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

    /// Connection to a JWT token retriever
    jwt: JwtRetriever,
}

impl C8YHttpProxyBuilder {
    pub fn new(
        config: C8YHttpConfig,
        http: &mut impl Service<HttpRequest, HttpResult>,
        jwt: &mut impl Service<(), JwtResult>,
    ) -> Self {
        let clients = ServerMessageBoxBuilder::new("C8Y-REST", 10);
        let http = ClientMessageBox::new(http);
        let jwt = JwtRetriever::new(jwt);
        C8YHttpProxyBuilder {
            config,
            clients,
            http,
            jwt,
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
            jwt: self.jwt,
        };

        C8YHttpProxyActor::new(self.config, message_box)
    }
}

impl MessageSink<RequestEnvelope<C8YRestRequest, C8YRestResult>, NoConfig> for C8YHttpProxyBuilder {
    fn get_sender(&self) -> DynSender<RequestEnvelope<C8YRestRequest, C8YRestResult>> {
        self.clients.get_sender()
    }
}

impl RuntimeRequestSink for C8YHttpProxyBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.clients.get_signal_sender()
    }
}
