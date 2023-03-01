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
use tedge_config::C8yUrlSetting;
use tedge_config::ConfigSettingAccessor;
use tedge_config::DeviceIdSetting;
use tedge_config::TEdgeConfig;
use tedge_config::TEdgeConfigError;
use tedge_config::TmpPathSetting;
use tedge_http_ext::HttpRequest;
use tedge_http_ext::HttpResult;

mod actor;
pub mod credentials;
pub mod handle;
pub mod messages;

#[cfg(test)]
mod tests;

/// Configuration of C8Y REST API
#[derive(Default)]
pub struct C8YHttpConfig {
    pub c8y_host: String,
    pub device_id: String,
    pub tmp_dir: PathBuf,
}

impl TryFrom<&TEdgeConfig> for C8YHttpConfig {
    type Error = TEdgeConfigError;

    fn try_from(tedge_config: &TEdgeConfig) -> Result<Self, Self::Error> {
        let c8y_host = tedge_config.query(C8yUrlSetting)?;
        let device_id = tedge_config.query(DeviceIdSetting)?;
        let tmp_dir = tedge_config.query(TmpPathSetting)?.into();
        Ok(Self {
            c8y_host: c8y_host.into(),
            device_id,
            tmp_dir,
        })
    }
}

/// A proxy to C8Y REST API
///
/// This is an actor builder.
/// - `impl ServiceProvider<C8YRestRequest, C8YRestResult, NoConfig>`
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
        http: &mut impl ServiceProvider<HttpRequest, HttpResult, NoConfig>,
        jwt: &mut impl ServiceProvider<(), JwtResult, NoConfig>,
    ) -> Self {
        let clients = ServerMessageBoxBuilder::new("C8Y-REST", 10);
        let http = ClientMessageBox::new("C8Y-REST => HTTP", http, NoConfig);
        let jwt = JwtRetriever::new("C8Y-REST => JWT", jwt, NoConfig);
        C8YHttpProxyBuilder {
            config,
            clients,
            http,
            jwt,
        }
    }
}

impl Builder<(C8YHttpConfig, C8YHttpProxyMessageBox)> for C8YHttpProxyBuilder {
    type Error = Infallible;

    fn try_build(self) -> Result<(C8YHttpConfig, C8YHttpProxyMessageBox), Self::Error> {
        Ok(self.build())
    }

    fn build(self) -> (C8YHttpConfig, C8YHttpProxyMessageBox) {
        let actor = self.config;
        let message_box = C8YHttpProxyMessageBox {
            clients: self.clients.build(),
            http: self.http,
            jwt: self.jwt,
        };
        (actor, message_box)
    }
}

impl ServiceProvider<C8YRestRequest, C8YRestResult, NoConfig> for C8YHttpProxyBuilder {
    fn connect_with(
        &mut self,
        peer: &mut impl ServiceConsumer<C8YRestRequest, C8YRestResult>,
        config: NoConfig,
    ) {
        self.clients.connect_with(peer, config)
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
