use crate::server::AppData;
use crate::server::Server;
use crate::tokens::C8yTokenManager;
use async_trait::async_trait;
use c8y_api::http_proxy::C8yAuthRetriever;
use camino::Utf8PathBuf;
use futures::channel::mpsc;
use futures::StreamExt;
use std::convert::Infallible;
use std::net::IpAddr;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_config::tedge_toml::mapper_config::C8yMapperConfig;
use tedge_config::TEdgeConfig;
use tedge_config_macros::OptionalConfig;
use tracing::info;

type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

pub struct C8yAuthProxyBuilder {
    app_data: AppData,
    bind_address: IpAddr,
    bind_port: u16,
    signal_sender: mpsc::Sender<RuntimeRequest>,
    signal_receiver: mpsc::Receiver<RuntimeRequest>,
    cert_path: OptionalConfig<Utf8PathBuf>,
    key_path: OptionalConfig<Utf8PathBuf>,
    ca_path: OptionalConfig<Utf8PathBuf>,
}

impl C8yAuthProxyBuilder {
    pub fn try_from_config(config: &TEdgeConfig, c8y: &C8yMapperConfig) -> anyhow::Result<Self> {
        let reqwest_client = config.cloud_root_certs()?.client();
        let auth_retriever = C8yAuthRetriever::from_tedge_config(config, c8y)?;
        let c8y = &c8y.cloud_specific;
        let app_data = AppData {
            is_https: true,
            host: c8y.http.to_string(),
            token_manager: C8yTokenManager::new(auth_retriever).shared(),
            client: reqwest_client,
        };
        let bind = &c8y.proxy.bind;
        let (signal_sender, signal_receiver) = mpsc::channel(10);
        let cert_path = c8y.proxy.cert_path.clone().map(Utf8PathBuf::from);
        let key_path = c8y.proxy.key_path.clone().map(Utf8PathBuf::from);
        let ca_path = c8y.proxy.ca_path.clone().map(Utf8PathBuf::from);

        Ok(Self {
            app_data,
            bind_address: bind.address,
            bind_port: bind.port,
            signal_sender,
            signal_receiver,
            cert_path,
            key_path,
            ca_path,
        })
    }
}

impl Builder<C8yAuthProxy> for C8yAuthProxyBuilder {
    type Error = Infallible;

    fn try_build(self) -> Result<C8yAuthProxy, Self::Error> {
        Ok(C8yAuthProxy {
            app_data: self.app_data,
            bind_address: self.bind_address,
            bind_port: self.bind_port,
            signal_receiver: self.signal_receiver,
            cert_path: self.cert_path,
            key_path: self.key_path,
            ca_path: self.ca_path,
        })
    }
}

impl RuntimeRequestSink for C8yAuthProxyBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        Box::new(self.signal_sender.clone())
    }
}

pub struct C8yAuthProxy {
    app_data: AppData,
    bind_address: IpAddr,
    bind_port: u16,
    signal_receiver: mpsc::Receiver<RuntimeRequest>,
    cert_path: OptionalConfig<Utf8PathBuf>,
    key_path: OptionalConfig<Utf8PathBuf>,
    ca_path: OptionalConfig<Utf8PathBuf>,
}

#[async_trait]
impl Actor for C8yAuthProxy {
    fn name(&self) -> &str {
        "C8yAuthProxy"
    }

    async fn run(mut self) -> Result<(), RuntimeError> {
        let server = Server::try_init(
            self.app_data,
            self.bind_address,
            self.bind_port,
            self.cert_path,
            self.key_path,
            self.ca_path,
        )
        .map_err(BoxError::from)?
        .wait();
        tokio::select! {
            result = server => {
                info!("Done");
                Ok(result.map_err(BoxError::from)?)
            },
            Some(RuntimeRequest::Shutdown) = self.signal_receiver.next() => {
                info!("Shutdown");
                Ok(())
            }
        }
    }
}
