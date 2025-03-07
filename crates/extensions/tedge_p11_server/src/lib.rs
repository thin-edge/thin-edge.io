pub mod service;
pub use service::P11Service;

pub mod client;

pub mod pkcs11;

pub mod signer;
pub mod single_cert_and_key;

mod p11_grpc {
    tonic::include_proto!("p11_grpc");
}
pub use p11_grpc::p11_server::P11Server;
