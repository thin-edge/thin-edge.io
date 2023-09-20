use std::convert::Infallible;
use std::net::IpAddr;

use axum::async_trait;
use c8y_http_proxy::credentials::C8YJwtRetriever;
use c8y_http_proxy::credentials::JwtRetriever;
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
use tedge_config::ConfigNotSet;
use tedge_config::TEdgeConfig;
use tracing::info;

use crate::server::AppState;
use crate::server::Server;
use crate::tokens::TokenManager;

type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

pub struct C8yAuthProxyBuilder {
    app_state: AppState,
    bind_address: IpAddr,
    bind_port: u16,
    signal_sender: mpsc::Sender<RuntimeRequest>,
    signal_receiver: mpsc::Receiver<RuntimeRequest>,
}

impl C8yAuthProxyBuilder {
    pub fn try_from_config(
        config: &TEdgeConfig,
        jwt: &mut ServerActorBuilder<C8YJwtRetriever, Sequential>,
    ) -> Result<Self, ConfigNotSet> {
        let app_state = AppState {
            target_host: format!("https://{}", config.c8y.http.or_config_not_set()?).into(),
            token_manager: TokenManager::new(JwtRetriever::new("C8Y-PROXY => JWT", jwt)).shared(),
        };
        let bind = &config.c8y.proxy.bind;
        let (signal_sender, signal_receiver) = mpsc::channel(10);

        Ok(Self {
            app_state,
            bind_address: bind.address,
            bind_port: bind.port,
            signal_sender,
            signal_receiver,
        })
    }
}

impl Builder<C8yAuthProxy> for C8yAuthProxyBuilder {
    type Error = Infallible;

    fn try_build(self) -> Result<C8yAuthProxy, Self::Error> {
        Ok(C8yAuthProxy {
            app_state: self.app_state,
            bind_address: self.bind_address,
            bind_port: self.bind_port,
            signal_receiver: self.signal_receiver,
        })
    }
}

impl RuntimeRequestSink for C8yAuthProxyBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        Box::new(self.signal_sender.clone())
    }
}

pub struct C8yAuthProxy {
    app_state: AppState,
    bind_address: IpAddr,
    bind_port: u16,
    signal_receiver: mpsc::Receiver<RuntimeRequest>,
}

#[async_trait]
impl Actor for C8yAuthProxy {
    fn name(&self) -> &str {
        "C8yAuthProxy"
    }

    async fn run(&mut self) -> Result<(), RuntimeError> {
        let server = Server::try_init(self.app_state.clone(), self.bind_address, self.bind_port)
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
