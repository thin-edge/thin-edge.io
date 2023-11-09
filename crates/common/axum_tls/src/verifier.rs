use rustls::server::ClientCertVerified;
use rustls::server::ClientCertVerifier;
use rustls::Certificate;
use rustls::DistinguishedName;
use std::time::SystemTime;

#[derive(Debug, Copy, Clone)]
/// An alternative to [AllowAnyAnonymousOrAuthenticatedClient](rustls::server::AllowAnyAnonymousOrAuthenticatedClient)
/// that doesn't attempt any client authentication
///
/// This prevents clients that are using certificates from having their connection rejected due to the
/// supplied certificate not being trusted
pub struct AllowAnyClient;

impl ClientCertVerifier for AllowAnyClient {
    fn offer_client_auth(&self) -> bool {
        false
    }

    fn client_auth_root_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        _: &Certificate,
        _: &[Certificate],
        _: SystemTime,
    ) -> Result<ClientCertVerified, rustls::Error> {
        unimplemented!("Client certificate verification is not supported by {self:?}, so this method is never called by rustls")
    }
}
