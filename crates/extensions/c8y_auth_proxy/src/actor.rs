use std::convert::Infallible;
use std::net::IpAddr;

use axum::async_trait;
use c8y_http_proxy::credentials::C8YJwtRetriever;
use c8y_http_proxy::credentials::JwtRetriever;
use camino::Utf8PathBuf;
use futures::channel::mpsc;
use futures::StreamExt;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::Sequential;
use tedge_actors::ServerActorBuilder;
use tedge_config::TEdgeConfig;
use tedge_config_macros::OptionalConfig;
use tracing::info;

use crate::server::AppData;
use crate::server::Server;
use crate::tokens::TokenManager;

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
    pub fn try_from_config(
        config: &TEdgeConfig,
        jwt: &mut ServerActorBuilder<C8YJwtRetriever, Sequential>,
    ) -> anyhow::Result<Self> {
        let reqwest_client = config.cloud_root_certs().client_builder().build().unwrap();
        let app_data = AppData {
            is_https: true,
            host: config.c8y.http.or_config_not_set()?.to_string(),
            token_manager: TokenManager::new(JwtRetriever::new(jwt)).shared(),
            client: reqwest_client,
        };
        let bind = &config.c8y.proxy.bind;
        let (signal_sender, signal_receiver) = mpsc::channel(10);
        let cert_path = config.c8y.proxy.cert_path.clone();
        let key_path = config.c8y.proxy.key_path.clone();
        let ca_path = config.c8y.proxy.ca_path.clone();

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
