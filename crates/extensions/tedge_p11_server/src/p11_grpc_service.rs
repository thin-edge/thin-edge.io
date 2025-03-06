use certificate::parse_root_certificate::pkcs11;
use certificate::parse_root_certificate::pkcs11::PkcsSigner;
use certificate::parse_root_certificate::CryptokiConfigDirect;
use tonic::{Request, Response, Status};

pub mod p11_grpc {
    tonic::include_proto!("p11_grpc");
}
use p11_grpc::{
    p11_server::P11, ChooseSchemeRequest, ChooseSchemeResponse, SignRequest, SignResponse,
};

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
        // TODO: implement properly!
        Ok(Response::new(ChooseSchemeResponse {
            chosen: "ECDSA".to_string(),
        }))
    }

    async fn sign(&self, request: Request<SignRequest>) -> Result<Response<SignResponse>, Status> {
        let message = request.into_inner().data;
        let signature = self.signer.sign(&message).unwrap();

        Ok(Response::new(SignResponse { signature }))
    }
}
