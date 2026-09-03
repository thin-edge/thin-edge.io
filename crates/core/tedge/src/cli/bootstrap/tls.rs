//! Diagnosing TLS trust failures against the cloud endpoint
//!
//! A request rejected by certificate verification is not a transient
//! failure: the MQTT bridge and the HTTP proxy verify the platform
//! against the same trust store, so a run that continues past it
//! leaves a device that looks bootstrapped and fails at connect time
//! with a far less legible error. Bootstrap therefore stops on such a
//! failure - naming the cause, the trust store in effect, and the fix.
//!
//! The cause matters: the failures below have different remedies,
//! and most of them are the *device's* fault rather than the platform's
//! (a missing CA, or a clock that has never been set).
//!
//! There is deliberately no flag to accept an untrusted certificate:
//! the way to bootstrap against a private CA without pre-installing it
//! is to trust the presented chain on first use - pinning it, so later
//! runs verify against something - which would attach here, to the
//! `UnknownIssuer` case, rather than being a blanket opt-out.

use anyhow::anyhow;
use std::error::Error;
use std::fmt::Write as _;

/// Why the peer certificate was rejected
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsFailureKind {
    /// The chain is not signed by any CA in the device's trust store
    UnknownIssuer,
    /// The certificate is past its `notAfter` date -
    /// or the device clock is ahead of it
    Expired,
    /// The certificate is before its `notBefore` date -
    /// on a device without a real-time clock, the clock is simply unset
    NotValidYet,
    /// The certificate does not cover the host that was requested
    NotValidForName,
    /// Another certificate verification failure
    Other,
}

/// A certificate verification failure, with the message that revealed it
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TlsTrustFailure {
    pub kind: TlsFailureKind,
    /// The TLS error as rendered by rustls,
    /// e.g. "invalid peer certificate: UnknownIssuer"
    pub detail: String,
}

/// The trust store a cloud instance verifies the platform against
pub struct TrustStore {
    /// The tedge config key, profile-qualified (e.g. `c8y.root_cert_path`)
    pub key: String,
    /// The file or directory the key points at
    pub path: String,
}

impl TrustStore {
    #[cfg(test)]
    pub fn test_value() -> Self {
        Self {
            key: "c8y.root_cert_path".to_owned(),
            path: "/etc/ssl/certs".to_owned(),
        }
    }
}

/// The error to report when a request to `host` failed certificate verification
///
/// `None` when the request failed for any other reason (DNS, timeout,
/// connection refused, an HTTP status): those stay recoverable.
pub fn tls_trust_error(
    err: &(dyn Error + 'static),
    host: &str,
    trust_store: &TrustStore,
) -> Option<anyhow::Error> {
    let failure = tls_trust_failure(err)?;
    Some(anyhow!("{}", failure.explain(host, trust_store)))
}

/// Classify an error as a certificate verification failure, if it is one
pub fn tls_trust_failure(err: &(dyn Error + 'static)) -> Option<TlsTrustFailure> {
    // rustls errors reach us wrapped in an io::Error, and io::Error::source
    // returns the *inner* error's source - skipping the inner error itself.
    // Walking `source()` alone would therefore never see the rustls error:
    // each io::Error on the way has to be unwrapped with `get_ref()`.
    let mut next = Some(err);
    while let Some(err) = next {
        if let Some(rustls_err) = err
            .downcast_ref::<std::io::Error>()
            .and_then(|io| io.get_ref())
            .and_then(|inner| inner.downcast_ref::<rustls::Error>())
        {
            if let rustls::Error::InvalidCertificate(cert_err) = rustls_err {
                return Some(TlsTrustFailure {
                    kind: classify(cert_err),
                    detail: rustls_err.to_string(),
                });
            }
            return None;
        }
        next = err.source();
    }

    // Not every client hands the rustls error through untouched
    // (it may be boxed, or flattened into a message):
    // fall back to the rendered chain, which keeps rustls' own wording
    from_message(&chain_message(err))
}

fn classify(err: &rustls::CertificateError) -> TlsFailureKind {
    use rustls::CertificateError::*;
    match err {
        UnknownIssuer => TlsFailureKind::UnknownIssuer,
        Expired | ExpiredContext { .. } => TlsFailureKind::Expired,
        NotValidYet | NotValidYetContext { .. } => TlsFailureKind::NotValidYet,
        NotValidForName | NotValidForNameContext { .. } => TlsFailureKind::NotValidForName,
        _ => TlsFailureKind::Other,
    }
}

/// Recognise a rustls certificate error from its rendered form
fn from_message(message: &str) -> Option<TlsTrustFailure> {
    let marker = "invalid peer certificate";
    let start = message.to_lowercase().find(marker)?;
    let detail = message[start..].to_owned();
    let lowercase = detail.to_lowercase();
    let contains = |needle: &str| lowercase.contains(needle);
    let kind = if contains("unknownissuer") {
        TlsFailureKind::UnknownIssuer
    } else if contains("expiredcontext") || contains("certificate expired") || contains("expired") {
        TlsFailureKind::Expired
    } else if contains("notvalidyet") || contains("certificate not valid yet") {
        TlsFailureKind::NotValidYet
    } else if contains("notvalidforname") || contains("certificate not valid for name") {
        TlsFailureKind::NotValidForName
    } else {
        TlsFailureKind::Other
    };
    Some(TlsTrustFailure { kind, detail })
}

/// Render an error and its sources as a single line
fn chain_message(err: &(dyn Error + 'static)) -> String {
    let mut message = err.to_string();
    let mut next = err.source();
    while let Some(err) = next {
        let source = err.to_string();
        // io::Error and friends already render their inner error
        if !message.contains(&source) {
            let _ = write!(message, ": {source}");
        }
        next = err.source();
    }
    message
}

impl TlsTrustFailure {
    /// What failed, in one line
    pub fn headline(&self, host: &str) -> String {
        match self.kind {
            TlsFailureKind::UnknownIssuer => {
                format!("The device does not trust the TLS certificate of {host}")
            }
            TlsFailureKind::Expired | TlsFailureKind::NotValidYet => {
                format!("The TLS certificate of {host} is outside its validity period")
            }
            TlsFailureKind::NotValidForName => {
                format!("The TLS certificate of {host} is not valid for that host name")
            }
            TlsFailureKind::Other => format!("The TLS certificate of {host} was rejected"),
        }
    }

    /// The headline with the TLS error, for contexts that only note the problem
    pub fn summary(&self, host: &str) -> String {
        format!("{} ({})", self.headline(host), self.detail)
    }

    /// The operator-facing report: what failed, why bootstrap stops here,
    /// and the commands that fix it
    pub fn explain(&self, host: &str, trust_store: &TrustStore) -> String {
        let TrustStore { key, path } = trust_store;
        let detail = &self.detail;
        let headline = self.headline(host);
        let same_store =
            "\nThe MQTT bridge and the HTTP proxy verify the platform against the same\n\
             trust store, so `tedge connect` would fail the same way.";
        let retry = "Then re-run the same command.\n\
             To configure this device without reaching the cloud, re-run with --offline.";
        match self.kind {
            TlsFailureKind::UnknownIssuer => format!(
                "{headline}\n\
                 \n\
                 Its certificate is signed by an issuer that is not in the device's trust\n\
                 store ({key} = {path}).\n\
                 {same_store}\n\
                 \n\
                 Install the CA certificate that signed the server certificate:\n\
                 \n\
                 \x20 # add it to the system trust store (Debian/Ubuntu)\n\
                 \x20 sudo cp <ca>.crt /usr/local/share/ca-certificates/ && sudo update-ca-certificates\n\
                 \n\
                 \x20 # or point thin-edge.io at a dedicated bundle\n\
                 \x20 sudo tedge config set {key} /path/to/ca.pem\n\
                 \n\
                 The certificates the server presents, to work out which CA is missing:\n\
                 \n\
                 \x20 openssl s_client -showcerts -connect {host}:443 </dev/null\n\
                 \n\
                 {retry}\n\
                 \n\
                 ({detail})"
            ),
            TlsFailureKind::Expired | TlsFailureKind::NotValidYet => {
                let cause = if self.kind == TlsFailureKind::Expired {
                    "The certificate is past its expiry date, or this device's clock is ahead of it."
                } else {
                    "The certificate is not valid yet: this device's clock is most likely behind -\n\
                     a device without a real-time clock starts up with an unset date."
                };
                format!(
                    "{headline}\n\
                     \n\
                     {detail}\n\
                     \n\
                     {cause}\n\
                     {same_store}\n\
                     \n\
                     Check the device clock, and correct it if it is wrong:\n\
                     \n\
                     \x20 date -u\n\
                     \x20 sudo chronyc makestep      # or: sudo date -s '<UTC date>'\n\
                     \n\
                     {retry}"
                )
            }
            TlsFailureKind::NotValidForName => format!(
                "{headline}\n\
                 \n\
                 {detail}\n\
                 \n\
                 Either the URL is wrong, or the connection is being intercepted by a\n\
                 TLS-inspecting proxy presenting its own certificate. Check the URL, and\n\
                 inspect the certificate the device is served:\n\
                 \n\
                 \x20 openssl s_client -showcerts -connect {host}:443 </dev/null\n\
                 \n\
                 {retry}"
            ),
            TlsFailureKind::Other => format!(
                "{headline}\n\
                 \n\
                 {detail}\n\
                 \n\
                 The certificate could not be verified against the device's trust store\n\
                 ({key} = {path}).\n\
                 {same_store}\n\
                 \n\
                 Inspect the certificate the device is served:\n\
                 \n\
                 \x20 openssl s_client -showcerts -connect {host}:443 </dev/null\n\
                 \n\
                 {retry}"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustls::CertificateError;

    /// An error wrapping another, the way a client wraps a transport error
    #[derive(Debug)]
    struct Wrapper(Box<dyn Error + Send + Sync + 'static>);

    impl std::fmt::Display for Wrapper {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "error sending request: {}", self.0)
        }
    }

    impl Error for Wrapper {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            Some(self.0.as_ref())
        }
    }

    fn transport_error(cert_err: CertificateError) -> Wrapper {
        let rustls_err = rustls::Error::InvalidCertificate(cert_err);
        let io = std::io::Error::new(std::io::ErrorKind::InvalidData, rustls_err);
        Wrapper(Box::new(io))
    }

    #[test]
    fn unknown_issuer_is_recognised_through_the_source_chain() {
        let err = transport_error(CertificateError::UnknownIssuer);
        let failure = tls_trust_failure(&err).expect("a certificate failure");
        assert_eq!(failure.kind, TlsFailureKind::UnknownIssuer);
        assert_eq!(failure.detail, "invalid peer certificate: UnknownIssuer");
    }

    #[test]
    fn expiry_and_clock_failures_are_told_apart() {
        for (cert_err, expected) in [
            (CertificateError::Expired, TlsFailureKind::Expired),
            (CertificateError::NotValidYet, TlsFailureKind::NotValidYet),
            (
                CertificateError::NotValidForName,
                TlsFailureKind::NotValidForName,
            ),
            (CertificateError::Revoked, TlsFailureKind::Other),
        ] {
            let err = transport_error(cert_err);
            let failure = tls_trust_failure(&err).expect("a certificate failure");
            assert_eq!(failure.kind, expected);
        }
    }

    #[test]
    fn other_tls_and_transport_errors_are_not_trust_failures() {
        let handshake =
            std::io::Error::new(std::io::ErrorKind::InvalidData, rustls::Error::DecryptError);
        assert_eq!(tls_trust_failure(&Wrapper(Box::new(handshake))), None);

        let refused = std::io::Error::from(std::io::ErrorKind::ConnectionRefused);
        assert_eq!(tls_trust_failure(&Wrapper(Box::new(refused))), None);
    }

    /// The rustls error is not always handed through as a typed error
    #[test]
    fn a_flattened_error_message_is_still_recognised() {
        #[derive(Debug)]
        struct Flattened(String);
        impl std::fmt::Display for Flattened {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }
        impl Error for Flattened {}

        let err =
            Flattened("client error (Connect): invalid peer certificate: UnknownIssuer".to_owned());
        let failure = tls_trust_failure(&err).expect("a certificate failure");
        assert_eq!(failure.kind, TlsFailureKind::UnknownIssuer);
    }

    #[test]
    fn the_report_names_the_trust_store_and_the_fix() {
        let err = transport_error(CertificateError::UnknownIssuer);
        let report = tls_trust_failure(&err)
            .unwrap()
            .explain("example.cumulocity.com", &TrustStore::test_value());
        assert!(report.contains("example.cumulocity.com"));
        assert!(report.contains("c8y.root_cert_path = /etc/ssl/certs"));
        assert!(report.contains("tedge config set c8y.root_cert_path"));
        assert!(report.contains("--offline"));
        // the wrapped literals must not leak their source indentation
        assert!(
            report.lines().all(|line| !line.starts_with("   ")),
            "unexpected indentation:\n{report}"
        );
    }

    #[test]
    fn the_one_line_form_keeps_the_host_and_the_cause() {
        let err = transport_error(CertificateError::UnknownIssuer);
        let summary = tls_trust_failure(&err)
            .unwrap()
            .summary("example.cumulocity.com");
        assert_eq!(
            summary,
            "The device does not trust the TLS certificate of example.cumulocity.com \
             (invalid peer certificate: UnknownIssuer)"
        );
    }

    #[test]
    fn the_clock_report_points_at_the_device_clock() {
        let err = transport_error(CertificateError::NotValidYet);
        let report = tls_trust_failure(&err)
            .unwrap()
            .explain("example.cumulocity.com", &TrustStore::test_value());
        assert!(report.contains("date -u"));
        assert!(report.contains("real-time clock"));
    }
}
