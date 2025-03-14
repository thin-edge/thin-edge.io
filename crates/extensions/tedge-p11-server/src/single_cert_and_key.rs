use std::sync::Arc;

use rustls::client::ResolvesClientCert;
use rustls::sign::CertifiedKey;
use rustls::SignatureScheme;

/// A clone of `rustls::sign::SingleCertAndKey`, but without the additional cert v3 check.
///
/// rustls previously allowed usage of v1 certificates with client authentication, but they
/// introduced v3 check in `SingleCertAndKey` resolver. Adding this check can break some user
/// setups, so we avoid using it for now.
#[derive(Debug)]
pub struct SingleCertAndKey(Arc<CertifiedKey>);

impl From<CertifiedKey> for SingleCertAndKey {
    fn from(certified_key: CertifiedKey) -> Self {
        Self(Arc::new(certified_key))
    }
}

impl ResolvesClientCert for SingleCertAndKey {
    fn resolve(
        &self,
        _root_hint_subjects: &[&[u8]],
        _sigschemes: &[SignatureScheme],
    ) -> Option<Arc<CertifiedKey>> {
        Some(Arc::clone(&self.0))
    }

    fn has_certs(&self) -> bool {
        true
    }
}
