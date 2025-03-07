//! A rustls signing key that connects to the tedge-p11-server UNIX socket for signing.

use hyper_util::rt::TokioIo;
use tokio::net::UnixStream;
use tonic::transport::{Channel, Endpoint};
use tonic::Request;
use tower::service_fn;

use crate::p11_grpc::p11_client::P11Client;
use crate::p11_grpc::{ChooseSchemeRequest, SignRequest};

pub async fn connect(socket_path: Arc<Path>) -> anyhow::Result<P11Client<Channel>> {
    // https://github.com/hyperium/tonic/blob/master/examples/src/uds/client.rs
    let channel = Endpoint::try_from("http://[::]:50051")?
        .connect_with_connector(service_fn(move |_| {
            let socket_path = socket_path.clone();
            async { Ok::<_, std::io::Error>(TokioIo::new(UnixStream::connect(socket_path).await?)) }
        }))
        .await?;

    let client = P11Client::new(channel);

    Ok(client)
}

use std::path::Path;
use std::sync::Arc;

use rustls::sign::{Signer, SigningKey};
use rustls::SignatureAlgorithm;
use tracing::{debug, instrument, trace};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TedgeP11Client {
    pub socket_path: Arc<Path>,
}

impl TedgeP11Client {
    pub async fn choose_scheme_async(
        &self,
        // TODO(marcel): choosing the scheme is unimplemented
        _offered: &[rustls::SignatureScheme],
    ) -> Option<Box<dyn rustls::sign::Signer>> {
        trace!("Connecting to socket...");
        let mut client = connect(self.socket_path.clone()).await.unwrap();

        debug!("Connected to socket");

        let request = Request::new(ChooseSchemeRequest { offered: vec![] });

        let _response = client.choose_scheme(request).await.unwrap();

        debug!("Choose scheme complete");

        Some(Box::new(TedgeP11ClientSigner {
            socket_path: self.socket_path.clone(),
        }))
    }
}

impl SigningKey for TedgeP11Client {
    #[instrument]
    fn choose_scheme(
        &self,
        offered: &[rustls::SignatureScheme],
    ) -> Option<Box<dyn rustls::sign::Signer>> {
        std::thread::scope(|s| {
            let h = s.spawn(|| {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                rt.block_on(self.choose_scheme_async(offered))
            });
            h.join().unwrap()
        })
    }

    #[instrument]
    fn algorithm(&self) -> SignatureAlgorithm {
        SignatureAlgorithm::ECDSA
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TedgeP11ClientSigner {
    pub socket_path: Arc<Path>,
}

impl TedgeP11ClientSigner {
    async fn sign_async(&self, message: &[u8]) -> Result<Vec<u8>, rustls::Error> {
        let mut client = connect(self.socket_path.clone()).await.unwrap();
        debug!("Connected to socket");

        let request = Request::new(SignRequest {
            data: message.to_vec(),
        });

        let response = client.sign(request).await.unwrap();
        debug!("Sign complete");

        Ok(response.into_inner().signature)
    }
}

impl Signer for TedgeP11ClientSigner {
    #[instrument]
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, rustls::Error> {
        std::thread::scope(|s| {
            let h = s.spawn(|| {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                rt.block_on(self.sign_async(message))
            });
            h.join().unwrap()
        })
    }

    fn scheme(&self) -> rustls::SignatureScheme {
        rustls::SignatureScheme::ECDSA_NISTP256_SHA256
    }
}
