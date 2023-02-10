use crate::c8y_http_proxy::actor::C8YHttpProxyMessageBox;
use crate::c8y_http_proxy::credentials::JwtResult;
use crate::c8y_http_proxy::credentials::JwtRetriever;
use crate::c8y_http_proxy::messages::C8YRestRequest;
use crate::c8y_http_proxy::messages::C8YRestResult;
use std::path::PathBuf;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::MessageBoxPlug;
use tedge_actors::MessageBoxSocket;
use tedge_actors::NoConfig;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::ServiceMessageBoxBuilder;
use tedge_config::C8yUrlSetting;
use tedge_config::ConfigSettingAccessor;
use tedge_config::DeviceIdSetting;
use tedge_config::TEdgeConfig;
use tedge_config::TEdgeConfigError;
use tedge_config::TmpPathSetting;
use tedge_http_ext::HttpConnectionBuilder;
use tedge_http_ext::HttpHandle;
use try_traits::Infallible;

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

pub trait C8YConnectionBuilder: MessageBoxSocket<C8YRestRequest, C8YRestResult, NoConfig> {}

impl C8YConnectionBuilder for C8YHttpProxyBuilder {}

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
        jwt: &mut impl MessageBoxSocket<(), JwtResult, NoConfig>,
    ) -> Self {
        let clients = ServiceMessageBoxBuilder::new("C8Y-REST", 10);
        let http = HttpHandle::new("C8Y-REST => HTTP", http, NoConfig);
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

impl MessageBoxSocket<C8YRestRequest, C8YRestResult, NoConfig> for C8YHttpProxyBuilder {
    fn connect_with(
        &mut self,
        peer: &mut impl MessageBoxPlug<C8YRestRequest, C8YRestResult>,
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
