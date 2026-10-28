//! Detection of the domain names under which the Cumulocity tenant is reachable.
//!
//! A tenant can be reachable under several domain names, e.g. while a dedicated instance is
//! being renamed, both the old and the new domain point to the same server.
//! URLs received from Cumulocity can use any of these names,
//! and all of them have to be routed through the local auth proxy.
//!
//! These names are listed by the TLS certificate served by Cumulocity,
//! so they are detected without any extra configuration.

use backoff::backoff::Backoff;
use backoff::ExponentialBackoff;
use certificate::CloudHttpConfig;
use reqwest::tls::TlsInfo;
use reqwest::Url;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tracing::info;
use tracing::warn;
use x509_parser::error::X509Error;
use x509_parser::extensions::GeneralName;
use x509_parser::prelude::FromDer;
use x509_parser::prelude::X509Certificate;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MIN_RETRY_DELAY: Duration = Duration::from_secs(30);
const MAX_RETRY_DELAY: Duration = Duration::from_secs(3600);
const REFRESH_INTERVAL: Duration = Duration::from_secs(24 * 3600);

/// The domain names under which Cumulocity is served, detected at runtime
///
/// These are the DNS names, possibly wildcards, listed by the certificate served at `c8y.http`,
/// including `c8y.http` itself.
/// Clones share the same names, so names detected in the background
/// are seen by all the clones.
#[derive(Clone, Debug)]
pub struct DomainNames {
    names: Arc<watch::Sender<Arc<[String]>>>,
}

impl Default for DomainNames {
    fn default() -> Self {
        DomainNames {
            names: Arc::new(watch::Sender::new(Arc::from(vec![]))),
        }
    }
}

impl DomainNames {
    /// Replaces the names, which are stored in lowercase as URL hosts are
    pub fn set(&self, names: Vec<String>) {
        let names: Vec<_> = names.iter().map(|name| name.to_ascii_lowercase()).collect();
        self.names.send_replace(names.into());
    }

    pub fn names(&self) -> Arc<[String]> {
        self.names.borrow().clone()
    }

    #[cfg(test)]
    fn subscribe(&self) -> watch::Receiver<Arc<[String]>> {
        self.names.subscribe()
    }

    /// Detects once the domain names of the Cumulocity tenant at `c8y_host` (i.e. `c8y.http`)
    pub async fn detect(
        c8y_host: &str,
        http_config: &CloudHttpConfig,
    ) -> Result<Self, DomainNameError> {
        let domain_names = DomainNames::default();
        let detector = DomainNameDetector::new(c8y_host, http_config, domain_names.clone())?;
        domain_names.set(detector.detect().await?);
        Ok(domain_names)
    }

    /// Returns true if the host is on one of these domains
    ///
    /// Hosts are compared like `c8y.http`: the first label is ignored,
    /// so `iot.example.com`, `t12345.example.com` and `*.example.com` all stand for `example.com`.
    /// A name with fewer than three labels, e.g. `example.com`, only stands for itself:
    /// ignoring its first label would leave a top-level domain, matching unrelated hosts.
    pub fn includes_domain_of(&self, host: &str) -> bool {
        let names = self.names();
        names.iter().any(|name| name.eq_ignore_ascii_case(host))
            || includes_domain(&names, parent_domain(host))
    }

    /// Returns true if the connected host is the configured tenant, possibly on another domain
    ///
    /// This is the case when both hosts have the same tenant name (their first label),
    /// and the domain of the connected host is one of these domains, as during a domain rename.
    /// Unlike [`includes_domain_of`](Self::includes_domain_of), another tenant on the same
    /// domain does not match, even when a wildcard covers all the tenants of that domain.
    pub fn is_same_tenant_on_another_domain(
        &self,
        configured_host: &str,
        connected_host: &str,
    ) -> bool {
        match (tenant_name(configured_host), tenant_name(connected_host)) {
            (Some(configured), Some(connected)) => {
                configured.eq_ignore_ascii_case(connected)
                    && includes_domain(&self.names(), parent_domain(connected_host))
            }
            _ => false,
        }
    }
}

/// Returns true if one of the certificate names stands for the domain
fn includes_domain(names: &[String], domain: &str) -> bool {
    names
        .iter()
        .filter_map(|name| certificate_domain(name))
        .any(|name| name.eq_ignore_ascii_case(domain))
}

fn parent_domain(host: &str) -> &str {
    host.split_once('.').map_or(host, |(_, domain)| domain)
}

/// The domain a certificate name stands for, if it has at least three labels
fn certificate_domain(name: &str) -> Option<&str> {
    let domain = parent_domain(name);
    (domain != name && domain.contains('.')).then_some(domain)
}

fn tenant_name(host: &str) -> Option<&str> {
    host.split_once('.').map(|(name, _)| name)
}

/// Detects in the background the domain names of the Cumulocity tenant at `c8y_host` (i.e. `c8y.http`)
///
/// The detection is retried on failure, and refreshed every day or when requested
/// through the returned [`DomainNameRefresh`].
/// On failure, the previously detected names are kept.
pub fn spawn_detection(
    c8y_host: &str,
    http_config: &CloudHttpConfig,
    domain_names: DomainNames,
) -> DomainNameRefresh {
    let (refresh, refresh_requests) = watch::channel(());
    match DomainNameDetector::new(c8y_host, http_config, domain_names) {
        Ok(detector) => {
            tokio::spawn(detector.run(refresh_requests));
        }
        Err(err) => warn!("The Cumulocity tenant domain names will not be detected: {err}"),
    }
    refresh.into()
}

/// Requests the domain names to be detected again, without waiting for the daily refresh
///
/// The certificate of Cumulocity is only checked when connecting,
/// so a reconnection is a good time to check whether it lists new domain names.
#[derive(Debug)]
pub struct DomainNameRefresh(watch::Sender<()>);

impl DomainNameRefresh {
    pub fn request(&self) {
        self.0.send_replace(());
    }
}

impl From<watch::Sender<()>> for DomainNameRefresh {
    fn from(requests: watch::Sender<()>) -> Self {
        DomainNameRefresh(requests)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DomainNameError {
    #[error("Invalid Cumulocity host {host}: {source}")]
    InvalidHost {
        host: String,
        source: url::ParseError,
    },

    #[error("Failed to build an HTTP client: {0}")]
    HttpClient(#[source] reqwest::Error),

    #[error("Failed to connect to {url}: {source}")]
    Connect { url: Url, source: reqwest::Error },

    #[error("No server certificate received from {url}")]
    NoCertificate { url: Url },

    #[error("Invalid server certificate received from {url}: {source}")]
    InvalidCertificate { url: Url, source: X509Error },
}

/// Extracts the DNS names listed by the Subject Alternative Name extension of a DER certificate
fn certificate_dns_names(der: &[u8]) -> Result<Vec<String>, X509Error> {
    let (_, certificate) = X509Certificate::from_der(der)?;
    let Some(san) = certificate.subject_alternative_name()? else {
        return Ok(vec![]);
    };
    Ok(san
        .value
        .general_names
        .iter()
        .filter_map(|name| match name {
            GeneralName::DNSName(name) => Some(name.to_string()),
            _ => None,
        })
        .collect())
}

/// Periodically detects the domain names from the certificate served by Cumulocity
struct DomainNameDetector {
    client: reqwest::Client,
    url: Url,
    domain_names: DomainNames,
    retry: ExponentialBackoff,
    refresh_interval: Duration,
}

/// Retries after 30s, doubling up to 1h, with some randomization so that devices reconnecting
/// at the same time, e.g. after a Cumulocity outage, don't all retry at the same time
fn retry_backoff() -> ExponentialBackoff {
    ExponentialBackoff {
        // The first delay is the current interval, which is only set to the initial one on reset
        current_interval: MIN_RETRY_DELAY,
        initial_interval: MIN_RETRY_DELAY,
        multiplier: 2.0,
        max_interval: MAX_RETRY_DELAY,
        max_elapsed_time: None,
        ..Default::default()
    }
}

impl DomainNameDetector {
    fn new(
        c8y_host: &str,
        http_config: &CloudHttpConfig,
        domain_names: DomainNames,
    ) -> Result<Self, DomainNameError> {
        let url = format!("https://{c8y_host}/").parse().map_err(|source| {
            DomainNameError::InvalidHost {
                host: c8y_host.to_string(),
                source,
            }
        })?;
        // Redirects are not followed, as the certificate of another server
        // would list the domain names of that server.
        // Connections are not reused, as only a new connection receives the current certificate.
        let client = http_config
            .client_builder()
            .tls_info(true)
            .redirect(reqwest::redirect::Policy::none())
            .pool_idle_timeout(Duration::ZERO)
            .build()
            .map_err(DomainNameError::HttpClient)?;
        Ok(DomainNameDetector {
            client,
            url,
            domain_names,
            retry: retry_backoff(),
            refresh_interval: REFRESH_INTERVAL,
        })
    }

    /// Returns the DNS names listed by the certificate currently served by Cumulocity
    async fn detect(&self) -> Result<Vec<String>, DomainNameError> {
        // Only the TLS handshake matters: the response status is ignored
        let response = self
            .client
            .head(self.url.clone())
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await
            .map_err(|source| DomainNameError::Connect {
                url: self.url.clone(),
                source,
            })?;
        let der = response
            .extensions()
            .get::<TlsInfo>()
            .and_then(TlsInfo::peer_certificate)
            .ok_or_else(|| DomainNameError::NoCertificate {
                url: self.url.clone(),
            })?;
        certificate_dns_names(der).map_err(|source| DomainNameError::InvalidCertificate {
            url: self.url.clone(),
            source,
        })
    }

    async fn run(mut self, mut refresh_requests: watch::Receiver<()>) {
        loop {
            let delay = match self.refresh().await {
                Ok(()) => {
                    // The names have just been detected, so the requests received meanwhile are served
                    refresh_requests.mark_unchanged();
                    self.refresh_interval
                }
                Err(retry_delay) => retry_delay,
            };
            tokio::select! {
                _ = tokio::time::sleep(delay) => {}
                // Once all the senders are dropped, only the delay is left
                Ok(()) = refresh_requests.changed() => {}
            }
        }
    }

    /// Detects the domain names once, returning on failure how long to wait before retrying
    async fn refresh(&mut self) -> Result<(), Duration> {
        match self.detect().await {
            Ok(names) => {
                self.update_domain_names(names);
                self.retry.reset();
                Ok(())
            }
            Err(err) => {
                let delay = self
                    .retry
                    .next_backoff()
                    .expect("ExponentialBackoff has no max elapsed time");
                warn!(
                    "Failed to detect the Cumulocity tenant domain names, retrying in {}s: {err}",
                    delay.as_secs()
                );
                Err(delay)
            }
        }
    }

    fn update_domain_names(&self, names: Vec<String>) {
        let previous = self.domain_names.names();
        self.domain_names.set(names);
        let current = self.domain_names.names();
        if previous != current {
            info!("Cumulocity tenant reachable as: {}", current.join(", "));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use rustls::pki_types::PrivateKeyDer;
    use rustls::pki_types::PrivatePkcs8KeyDer;
    use tokio::io::AsyncReadExt;
    use tokio::io::AsyncWriteExt;
    use tokio::task::JoinHandle;

    #[test]
    fn domain_names_are_shared_by_clones() {
        let domain_names = DomainNames::default();
        let clone = domain_names.clone();
        assert!(clone.names().is_empty());

        domain_names.set(vec!["iot.new-domain.com".to_string()]);

        assert_eq!(*clone.names(), ["iot.new-domain.com".to_string()]);
    }

    #[test]
    fn domain_names_are_stored_in_lowercase() {
        let domain_names = DomainNames::default();

        domain_names.set(vec!["IoT.New-Domain.com".to_string()]);

        assert_eq!(*domain_names.names(), ["iot.new-domain.com".to_string()]);
    }

    #[rstest]
    #[case::exact_name("iot.new-domain.com", true)]
    #[case::other_name_on_the_domain("t12345.new-domain.com", true)]
    #[case::wildcard("t12345.wildcard-domain.com", true)]
    #[case::subdomain("a.iot.new-domain.com", false)]
    #[case::unrelated_domain("iot.unrelated.com", false)]
    #[case::bare_domain("apex-domain.com", true)]
    #[case::host_on_bare_domain("t12345.apex-domain.com", false)]
    #[case::unrelated_host_on_same_top_level_domain("github.com", false)]
    #[case::single_label_host("localhost", false)]
    fn hosts_are_compared_like_c8y_http(#[case] host: &str, #[case] expected: bool) {
        let domain_names = DomainNames::default();
        domain_names.set(vec![
            "iot.new-domain.com".to_string(),
            "*.wildcard-domain.com".to_string(),
            "apex-domain.com".to_string(),
            "com".to_string(),
        ]);

        assert_eq!(domain_names.includes_domain_of(host), expected);
    }

    #[rstest]
    #[case::renamed_domain("connect.old-domain.org", "connect.new-domain.com", true)]
    #[case::other_tenant_on_wildcard_domain(
        "connect.old-domain.org",
        "other.new-domain.com",
        false
    )]
    #[case::other_tenant_on_same_domain("connect.old-domain.org", "other.old-domain.org", false)]
    #[case::domain_not_on_certificate("connect.old-domain.org", "connect.unrelated.com", false)]
    #[case::top_level_domain_of_bare_domain("connect.old-domain.org", "connect.org", false)]
    #[case::bare_domain_on_certificate("old-domain.other.com", "old-domain.org", false)]
    #[case::single_label_hosts("localhost", "localhost", false)]
    fn same_tenant_on_another_domain(
        #[case] configured_host: &str,
        #[case] connected_host: &str,
        #[case] expected: bool,
    ) {
        let domain_names = DomainNames::default();
        domain_names.set(vec![
            "*.old-domain.org".to_string(),
            "*.new-domain.com".to_string(),
            "old-domain.org".to_string(),
        ]);

        assert_eq!(
            domain_names.is_same_tenant_on_another_domain(configured_host, connected_host),
            expected
        );
    }

    #[test]
    fn dns_names_are_extracted_from_certificate() {
        let names = vec![
            "iot.old-domain.com".to_string(),
            "*.new-domain.com".to_string(),
        ];
        let certificate = rcgen::generate_simple_self_signed(names.clone()).unwrap();

        let detected = certificate_dns_names(certificate.cert.der()).unwrap();

        assert_eq!(detected, names);
    }

    #[tokio::test]
    async fn domain_names_are_detected_from_the_certificate_served_by_cumulocity() {
        let names = vec![
            "localhost".to_string(),
            "iot.old-domain.com".to_string(),
            "*.new-domain.com".to_string(),
        ];
        let certificate = rcgen::generate_simple_self_signed(names.clone()).unwrap();
        let c8y = TestServer::start(&certificate, OK_RESPONSE).await;

        let domain_names = DomainNames::detect(&c8y.host, &trusting(&[&certificate]))
            .await
            .unwrap();

        assert_eq!(*domain_names.names(), names);
    }

    #[tokio::test]
    async fn domain_names_are_not_detected_from_the_certificate_of_a_redirect_target() {
        let names = vec!["localhost".to_string(), "iot.old-domain.com".to_string()];
        let certificate = rcgen::generate_simple_self_signed(names.clone()).unwrap();
        let other_names = vec!["localhost".to_string(), "login.unrelated.com".to_string()];
        let other_certificate = rcgen::generate_simple_self_signed(other_names).unwrap();
        let other = TestServer::start(&other_certificate, OK_RESPONSE).await;
        let redirect = format!(
            "HTTP/1.1 301 Moved Permanently\r\nlocation: https://{}/\r\ncontent-length: 0\r\n\r\n",
            other.host
        );
        let c8y = TestServer::start(&certificate, &redirect).await;
        let http_config = trusting(&[&certificate, &other_certificate]);

        let domain_names = DomainNames::detect(&c8y.host, &http_config).await.unwrap();

        assert_eq!(*domain_names.names(), names);
    }

    #[tokio::test]
    async fn each_detection_reads_the_certificate_currently_served() {
        let old_names = vec!["localhost".to_string(), "iot.old-domain.com".to_string()];
        let old_certificate = rcgen::generate_simple_self_signed(old_names.clone()).unwrap();
        let new_names = vec!["localhost".to_string(), "*.new-domain.com".to_string()];
        let new_certificate = rcgen::generate_simple_self_signed(new_names.clone()).unwrap();
        // The server keeps connections open, which the client must not reuse
        let c8y = TestServer::start(&old_certificate, OK_RESPONSE).await;
        let domain_names = DomainNames::default();
        let mut detector = DomainNameDetector::new(
            &c8y.host,
            &trusting(&[&old_certificate, &new_certificate]),
            domain_names.clone(),
        )
        .unwrap();
        detector.refresh().await.unwrap();
        assert_eq!(*domain_names.names(), old_names);

        c8y.serve(&new_certificate);
        detector.refresh().await.unwrap();

        assert_eq!(*domain_names.names(), new_names);
    }

    #[tokio::test]
    async fn domain_names_are_detected_again_on_request() {
        let old_names = vec!["localhost".to_string(), "iot.old-domain.com".to_string()];
        let old_certificate = rcgen::generate_simple_self_signed(old_names.clone()).unwrap();
        let new_names = vec!["localhost".to_string(), "*.new-domain.com".to_string()];
        let new_certificate = rcgen::generate_simple_self_signed(new_names.clone()).unwrap();
        let c8y = TestServer::start(&old_certificate, OK_RESPONSE).await;
        let domain_names = DomainNames::default();
        let detector = DomainNameDetector::new(
            &c8y.host,
            &trusting(&[&old_certificate, &new_certificate]),
            domain_names.clone(),
        )
        .unwrap();
        let (refresh, refresh_requests) = watch::channel(());
        let refresh = DomainNameRefresh::from(refresh);
        let detection = tokio::spawn(detector.run(refresh_requests));
        wait_for_names(&domain_names, &old_names).await;

        c8y.serve(&new_certificate);
        refresh.request();

        wait_for_names(&domain_names, &new_names).await;
        detection.abort();
    }

    #[tokio::test]
    async fn detection_fails_when_cumulocity_is_unreachable() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let http_config = CloudHttpConfig::test_value();

        let detector = DomainNameDetector::new(
            &format!("127.0.0.1:{port}"),
            &http_config,
            DomainNames::default(),
        )
        .unwrap();

        let err = detector.detect().await.unwrap_err();
        assert!(err.to_string().contains(&format!("127.0.0.1:{port}")));
    }

    #[tokio::test]
    async fn detected_domain_names_are_kept_when_cumulocity_becomes_unreachable() {
        let names = vec!["localhost".to_string(), "iot.new-domain.com".to_string()];
        let certificate = rcgen::generate_simple_self_signed(names.clone()).unwrap();
        let c8y = TestServer::start(&certificate, OK_RESPONSE).await;
        let domain_names = DomainNames::default();
        let mut detector = test_detector(&c8y, &certificate, &domain_names);
        detector.refresh().await.unwrap();

        c8y.refuse();

        assert_eq!(detector.refresh().await, Err(MIN_RETRY_DELAY));
        assert_eq!(*domain_names.names(), names);
    }

    #[tokio::test]
    async fn retries_start_again_from_the_minimum_delay_after_a_success() {
        let names = vec!["localhost".to_string()];
        let certificate = rcgen::generate_simple_self_signed(names).unwrap();
        let c8y = TestServer::start(&certificate, OK_RESPONSE).await;
        let domain_names = DomainNames::default();
        let mut detector = test_detector(&c8y, &certificate, &domain_names);

        c8y.refuse();
        assert_eq!(detector.refresh().await, Err(MIN_RETRY_DELAY));
        assert_eq!(detector.refresh().await, Err(MIN_RETRY_DELAY * 2));

        c8y.serve(&certificate);
        assert_eq!(detector.refresh().await, Ok(()));

        c8y.refuse();
        assert_eq!(detector.refresh().await, Err(MIN_RETRY_DELAY));
    }

    const OK_RESPONSE: &str = "HTTP/1.1 200 OK\r\ncontent-length: 0\r\n\r\n";

    // A detector whose retry delays are not randomized, so they can be checked
    fn test_detector(
        c8y: &TestServer,
        certificate: &rcgen::CertifiedKey<rcgen::KeyPair>,
        domain_names: &DomainNames,
    ) -> DomainNameDetector {
        let mut detector =
            DomainNameDetector::new(&c8y.host, &trusting(&[certificate]), domain_names.clone())
                .unwrap();
        detector.retry = ExponentialBackoff {
            randomization_factor: 0.0,
            ..retry_backoff()
        };
        detector
    }

    async fn wait_for_names(domain_names: &DomainNames, expected: &[String]) {
        let mut names = domain_names.subscribe();
        tokio::time::timeout(
            Duration::from_secs(5),
            names.wait_for(|names| **names == *expected),
        )
        .await
        .unwrap_or_else(|_| panic!("Domain names should be detected: {expected:?}"))
        .unwrap();
    }

    fn trusting(certificates: &[&rcgen::CertifiedKey<rcgen::KeyPair>]) -> CloudHttpConfig {
        let trusted = certificates
            .iter()
            .map(|certificate| reqwest::Certificate::from_der(certificate.cert.der()).unwrap())
            .collect::<Vec<_>>();
        CloudHttpConfig::new(trusted, None)
    }

    /// An HTTPS server standing for Cumulocity, sending the same response to every request
    ///
    /// The certificate it serves can be changed, and it can refuse connections,
    /// while it keeps listening on the same address.
    struct TestServer {
        host: String,
        config: Arc<std::sync::RwLock<Option<Arc<rustls::ServerConfig>>>>,
        task: JoinHandle<()>,
    }

    impl TestServer {
        async fn start(certificate: &rcgen::CertifiedKey<rcgen::KeyPair>, response: &str) -> Self {
            let _ = rustls::crypto::ring::default_provider().install_default();
            let config = Arc::new(std::sync::RwLock::new(Some(server_config(certificate))));
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let response = response.to_string();
            let current_config = config.clone();
            let task = tokio::spawn(async move {
                while let Ok((stream, _)) = listener.accept().await {
                    // A refused connection is closed before the TLS handshake
                    let Some(config) = current_config.read().unwrap().clone() else {
                        continue;
                    };
                    let response = response.clone();
                    tokio::spawn(async move {
                        let acceptor = tokio_rustls::TlsAcceptor::from(config);
                        let Ok(mut stream) = acceptor.accept(stream).await else {
                            return;
                        };
                        // Keep the connection open, serving each request received on it
                        let mut request = [0; 1024];
                        while let Ok(1..) = stream.read(&mut request).await {
                            if stream.write_all(response.as_bytes()).await.is_err() {
                                return;
                            }
                        }
                    });
                }
            });
            TestServer {
                host: format!("localhost:{port}"),
                config,
                task,
            }
        }

        fn serve(&self, certificate: &rcgen::CertifiedKey<rcgen::KeyPair>) {
            *self.config.write().unwrap() = Some(server_config(certificate));
        }

        fn refuse(&self) {
            *self.config.write().unwrap() = None;
        }
    }

    impl Drop for TestServer {
        fn drop(&mut self) {
            self.task.abort();
        }
    }

    fn server_config(
        certificate: &rcgen::CertifiedKey<rcgen::KeyPair>,
    ) -> Arc<rustls::ServerConfig> {
        let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
            certificate.signing_key.serialize_der(),
        ));
        let config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![certificate.cert.der().clone()], key)
            .unwrap();
        Arc::new(config)
    }
}
