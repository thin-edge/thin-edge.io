use crate::pkcs11;
use crate::pkcs11::CryptokiConfigDirect;
use crate::pkcs11::PkcsSigner;
use tonic::{Request, Response, Status};

use crate::p11_grpc::{
    p11_server::P11, ChooseSchemeRequest, ChooseSchemeResponse, SignRequest, SignResponse,
};
use tracing::debug;

#[derive(Debug)]
pub struct P11Service {
    signer: PkcsSigner,
}

impl P11Service {
    pub fn from_config(config: CryptokiConfigDirect) -> anyhow::Result<Self> {
        let signing_key = pkcs11::Pkcs11SigningKey::from_cryptoki_config(config)
            .expect("failed to get pkcs11 signing key");

        let session = match signing_key {
            pkcs11::Pkcs11SigningKey::Ecdsa(e) => e.pkcs11,
            _ => panic!("Expected a session"),
        };
        let signer = PkcsSigner::from_session(session);

        Ok(Self { signer })
    }
}

#[tonic::async_trait]
impl P11 for P11Service {
    async fn choose_scheme(
        &self,
        _request: Request<ChooseSchemeRequest>,
    ) -> Result<Response<ChooseSchemeResponse>, Status> {
        debug!("choose scheme");
        // TODO: implement properly!
        Ok(Response::new(ChooseSchemeResponse {
            chosen: "ECDSA".to_string(),
        }))
    }

    async fn sign(&self, request: Request<SignRequest>) -> Result<Response<SignResponse>, Status> {
        debug!("sign");
        let message = request.into_inner().data;
        let signature = self.signer.sign(&message).unwrap();

        Ok(Response::new(SignResponse { signature }))
    }
}
