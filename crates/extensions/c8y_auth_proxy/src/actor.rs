use std::convert::Infallible;
use std::net::IpAddr;

use axum::async_trait;
use c8y_http_proxy::credentials::C8YJwtRetriever;
use c8y_http_proxy::credentials::JwtRetriever;
use camino::Utf8PathBuf;
use futures::channel::mpsc;
use futures::StreamExt;
use miette::miette;
use miette::Context;
use miette::IntoDiagnostic;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::Sequential;
use tedge_actors::ServerActorBuilder;
use tedge_config::ReadableKey::C8yProxyCertFile;
use tedge_config::ReadableKey::C8yProxyKeyFile;
use tedge_config::ReadableKey::DeviceCertPath;
use tedge_config::ReadableKey::DeviceKeyPath;
use tedge_config::TEdgeConfig;
use tracing::info;

use crate::server::AppState;
use crate::server::Server;
use crate::tls::load_cert;
use crate::tls::load_pkey;
use crate::tokens::TokenManager;

type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

pub struct C8yAuthProxyBuilder {
    app_state: AppState,
    bind_address: IpAddr,
    bind_port: u16,
    certificate_der: Vec<Vec<u8>>,
    key_der: Vec<u8>,
    ca_dir: Option<Utf8PathBuf>,
    signal_sender: mpsc::Sender<RuntimeRequest>,
    signal_receiver: mpsc::Receiver<RuntimeRequest>,
}

impl C8yAuthProxyBuilder {
    pub fn try_from_config(
        config: &TEdgeConfig,
        jwt: &mut ServerActorBuilder<C8YJwtRetriever, Sequential>,
    ) -> miette::Result<Self> {
        let app_state = AppState {
            target_host: format!(
                "https://{}",
                config.c8y.http.or_config_not_set().into_diagnostic()?
            )
            .into(),
            token_manager: TokenManager::new(JwtRetriever::new("C8Y-PROXY => JWT", jwt)).shared(),
        };
        let bind = &config.c8y.proxy.bind;
        let (signal_sender, signal_receiver) = mpsc::channel(10);
        let (certificate_der, key_der) = load_certificate_and_key(config)?;

        let ca_dir = config.mqtt.external.ca_path.or_none().map(<_>::to_owned);

        Ok(Self {
            app_state,
            bind_address: bind.address,
            bind_port: bind.port,
            signal_sender,
            signal_receiver,
            certificate_der,
            key_der,
            ca_dir,
        })
    }
}

fn load_certificate_and_key(config: &TEdgeConfig) -> miette::Result<(Vec<Vec<u8>>, Vec<u8>)> {
    let paths = tedge_config_macros::all_or_nothing((
        config.c8y.proxy.cert_file.as_ref(),
        config.c8y.proxy.key_file.as_ref(),
    ))
    .map_err(|e| miette!("{e}"))?;

    match paths {
        Some((external_cert_file, external_key_file)) => Ok((
            load_cert(external_cert_file).with_context(|| {
                format!("reading certificate configured in {C8yProxyCertFile:?}")
            })?,
            load_pkey(external_key_file).with_context(|| {
                format!("reading private key configured in `{C8yProxyKeyFile}`")
            })?,
        )),
        None => Ok((
            load_cert(&config.device.cert_path)
                .with_context(|| format!("reading certificate configured in `{DeviceCertPath}`"))?,
            load_pkey(&config.device.key_path)
                .with_context(|| format!("reading private key configured in `{DeviceKeyPath}`"))?,
        )),
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
            certificate_der: self.certificate_der,
            key_der: self.key_der,
            ca_dir: self.ca_dir,
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
    certificate_der: Vec<Vec<u8>>,
    key_der: Vec<u8>,
    ca_dir: Option<Utf8PathBuf>,
    signal_receiver: mpsc::Receiver<RuntimeRequest>,
}

#[async_trait]
impl Actor for C8yAuthProxy {
    fn name(&self) -> &str {
        "C8yAuthProxy"
    }

    async fn run(mut self) -> Result<(), RuntimeError> {
        let server = Server::try_init(
            self.app_state,
            self.bind_address,
            self.bind_port,
            self.certificate_der,
            self.key_der,
            self.ca_dir,
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
