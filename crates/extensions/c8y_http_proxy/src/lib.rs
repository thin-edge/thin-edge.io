use crate::actor::C8YHttpProxyActor;
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
use tedge_actors::ServiceProvider;
use tedge_config::new::ConfigNotSet;
use tedge_config::new::ReadError;
use tedge_config::new::TEdgeConfig as NewTEdgeConfig;
use tedge_config::C8yHttpSetting;
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

// This must be removed once we are done with moving to new tedge config API
impl TryFrom<&TEdgeConfig> for C8YHttpConfig {
    type Error = TEdgeConfigError;

    fn try_from(tedge_config: &TEdgeConfig) -> Result<Self, Self::Error> {
        let c8y_host = tedge_config.query(C8yHttpSetting)?;
        let device_id = tedge_config.query(DeviceIdSetting)?;
        let tmp_dir = tedge_config.query(TmpPathSetting)?.into();
        Ok(Self {
            c8y_host: c8y_host.into(),
            device_id,
            tmp_dir,
        })
    }
}

impl TryFrom<&NewTEdgeConfig> for C8YHttpConfig {
    type Error = C8yHttpConfigBuildError;

    fn try_from(tedge_config: &NewTEdgeConfig) -> Result<Self, Self::Error> {
        let c8y_host = tedge_config.c8y_url().or_config_not_set()?.to_string();
        let device_id = tedge_config.device.id.try_read(tedge_config)?.to_string();
        let tmp_dir = tedge_config.tmp.path.as_std_path().to_path_buf();

        Ok(Self {
            c8y_host,
            device_id,
            tmp_dir,
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
        let http = ClientMessageBox::new("C8Y-REST => HTTP", http);
        let jwt = JwtRetriever::new("C8Y-REST => JWT", jwt);
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

impl ServiceProvider<C8YRestRequest, C8YRestResult, NoConfig> for C8YHttpProxyBuilder {
    fn connect_consumer(
        &mut self,
        config: NoConfig,
        response_sender: DynSender<C8YRestResult>,
    ) -> DynSender<C8YRestRequest> {
        self.clients.connect_consumer(config, response_sender)
    }
}

impl RuntimeRequestSink for C8YHttpProxyBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.clients.get_signal_sender()
    }
}
