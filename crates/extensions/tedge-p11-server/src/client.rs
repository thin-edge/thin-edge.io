use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::Arc;

use anyhow::bail;
use rustls::sign::Signer;
use rustls::sign::SigningKey;
use tracing::debug;
use tracing::instrument;
use tracing::trace;

use crate::connection::Payload;

use super::connection::Frame;
use super::service::ChooseSchemeRequest;
use super::service::SignRequest;

#[derive(Debug, Clone, PartialEq, Eq)]
struct TedgeP11Client {
    pub socket_path: Arc<Path>,
}

impl TedgeP11Client {
    pub fn choose_scheme(
        &self,
        offered: &[rustls::SignatureScheme],
    ) -> anyhow::Result<Option<rustls::SignatureScheme>> {
        trace!("Connecting to socket...");
        let stream = UnixStream::connect(&self.socket_path)?;
        let mut connection = crate::connection::Connection::new(stream);

        debug!("Connected to socket");

        let request = Frame::new(Payload::ChooseSchemeRequest(ChooseSchemeRequest {
            offered: offered
                .iter()
                .copied()
                .map(super::service::SignatureScheme)
                .collect::<Vec<_>>(),
        }));
        connection.write_frame(&request)?;

        let response = connection.read_frame()?.payload;

        let Payload::ChooseSchemeResponse(response) = response else {
            bail!("protocol error: bad response, expected chose scheme");
        };

        debug!("Choose scheme complete");

        let Some(scheme) = response.scheme else {
            return Ok(None);
        };

        Ok(Some(scheme.0))
    }

    pub fn sign(&self, message: &[u8]) -> anyhow::Result<Vec<u8>> {
        let stream = UnixStream::connect(&self.socket_path)?;
        let mut connection = crate::connection::Connection::new(stream);
        debug!("Connected to socket");

        let request = Frame::new(Payload::SignRequest(SignRequest {
            to_sign: message.to_vec(),
        }));
        connection.write_frame(&request)?;

        let response = connection.read_frame()?.payload;

        let Payload::SignResponse(response) = response else {
            bail!("protocol error: bad response, expected sign");
        };

        debug!("Sign complete");

        Ok(response.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TedgeP11ClientSigningKey {
    pub socket_path: Arc<Path>,
}

impl SigningKey for TedgeP11ClientSigningKey {
    fn choose_scheme(
        &self,
        offered: &[rustls::SignatureScheme],
    ) -> Option<Box<dyn rustls::sign::Signer>> {
        let client = TedgeP11Client {
            socket_path: self.socket_path.clone(),
        };
        let response = client.choose_scheme(offered).unwrap();
        let scheme = response?;

        Some(Box::new(TedgeP11ClientSigner {
            socket_path: self.socket_path.clone(),
            scheme,
        }))
    }

    // TODO(marcel): algorithm
    fn algorithm(&self) -> rustls::SignatureAlgorithm {
        todo!()
        // let client = TedgeP11Client {
        //     socket_path: self.socket_path.clone(),
        // };
        // let response = client.choose_scheme(offered).unwrap();
        // let scheme = response.unwrap();

        // match scheme {
        //     SignatureScheme::RSA_PKCS1_SHA1
        //     | SignatureScheme::RSA_PKCS1_SHA256
        //     | SignatureScheme::RSA_PKCS1_SHA384
        //     | SignatureScheme::RSA_PKCS1_SHA512
        //     | SignatureScheme::RSA_PSS_SHA256
        //     | SignatureScheme::RSA_PSS_SHA384
        //     | SignatureScheme::RSA_PSS_SHA512 => SignatureAlgorithm::RSA,
        //     SignatureScheme::ECDSA_SHA1_Legacy
        //     | SignatureScheme::ECDSA_NISTP256_SHA256
        //     | SignatureScheme::ECDSA_NISTP384_SHA384
        //     | SignatureScheme::ECDSA_NISTP521_SHA512 => SignatureAlgorithm::ECDSA,
        //     SignatureScheme::ED25519 => SignatureAlgorithm::ED25519,
        //     SignatureScheme::ED448 => SignatureAlgorithm::ED448,
        //     _ => SignatureAlgorithm::Unknown(0),
        // }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TedgeP11ClientSigner {
    pub socket_path: Arc<Path>,
    scheme: rustls::SignatureScheme,
}

impl Signer for TedgeP11ClientSigner {
    #[instrument]
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, rustls::Error> {
        let client = TedgeP11Client {
            socket_path: self.socket_path.clone(),
        };
        let response = client.sign(message).unwrap();
        Ok(response)
    }

    fn scheme(&self) -> rustls::SignatureScheme {
        self.scheme
    }
}
