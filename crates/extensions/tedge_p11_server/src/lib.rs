use certificate::parse_root_certificate::pkcs11::{self, PkcsSigner};
use tracing::instrument;

mod p11_grpc_service;
pub use p11_grpc_service::p11_grpc::p11_server::P11Server;
pub use p11_grpc_service::P11Service;

#[derive(Debug)]
struct CryptokiResolverService {
    signer: pkcs11::PkcsSigner,
}

impl CryptokiResolverService {
    fn new(signing_key: pkcs11::Pkcs11SigningKey) -> Self {
        let session = match signing_key {
            pkcs11::Pkcs11SigningKey::Ecdsa(e) => e.pkcs11,
            _ => panic!("Expected a session"),
        };
        let signer = PkcsSigner::from_session(session);

        CryptokiResolverService { signer }
    }

    #[instrument]
    fn choose_scheme(&self, request: ChooseSchemeRequest) -> ChooseSchemeResponse {
        ChooseSchemeResponse {
            scheme: rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
        }
    }

    #[instrument]
    fn sign(&self, request: SignRequest) -> SignResponse {
        let signature = self.signer.sign(&request.to_sign).unwrap();
        SignResponse(signature)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ChooseSchemeRequest {
    offered: Vec<rustls::SignatureScheme>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ChooseSchemeResponse {
    scheme: rustls::SignatureScheme,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SignRequest {
    to_sign: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SignResponse(Vec<u8>);
