use core::fmt;
use std::str::FromStr;

use anyhow::bail;
use anyhow::ensure;
use serde::Deserialize;
use serde::Serialize;

use super::proxy_scheme::ProxyScheme;
use super::Port;

#[derive(Debug, PartialEq, Eq, Clone, Deserialize, Serialize)]
#[serde(try_from = "String", into = "String")]
pub struct ProxyUrl {
    scheme: ProxyScheme,
    host: url::Host,
    port: Port,
}

impl ProxyUrl {
    pub fn url(&self) -> url::Url {
        self.to_string().parse().unwrap()
    }

    pub fn host(&self) -> &url::Host {
        &self.host
    }

    pub fn port(&self) -> Port {
        self.port
    }

    pub fn scheme(&self) -> ProxyScheme {
        self.scheme
    }
}

impl fmt::Display for ProxyUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}://{}:{}", self.scheme, self.host, self.port)
    }
}

#[derive(thiserror::Error, Debug)]
#[error("Invalid proxy URL: {0:#}")]
pub struct InvalidProxyUrl(#[from] anyhow::Error);

impl TryFrom<String> for ProxyUrl {
    type Error = InvalidProxyUrl;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<ProxyUrl> for String {
    fn from(url: ProxyUrl) -> String {
        url.to_string()
    }
}

impl FromStr for ProxyUrl {
    type Err = InvalidProxyUrl;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (host, port, scheme) = parse_host_port(s)?;
        Ok(Self { scheme, host, port })
    }
}

fn parse_host_port(s: &str) -> anyhow::Result<(url::Host, Port, ProxyScheme)> {
    let url = url::Url::parse(s).or_else(|err| {
        // Make sure we're not doubling up the supplied scheme if there is one
        if !s.contains("://") {
            url::Url::parse(&format!("http://{s}"))
        } else {
            Err(err)
        }
    })?;
    ensure!(
        url.username() == "",
        "URL should not contain a username, please specify this in the dedicated configuration"
    );
    ensure!(
        url.password().is_none(),
        "URL should not contain a password, please specify this in the dedicated configuration"
    );
    let scheme = match url.scheme() {
        "http" => ProxyScheme::Http,
        "https" => ProxyScheme::Https,
        other => bail!("Unsupported proxy scheme: {other}"),
    };
    match (url.host(), url.port_or_known_default()) {
        (Some(host), Some(port)) => Ok((host.to_owned(), Port(port), scheme)),
        (None, _) => {
            unreachable!("Host cannot be empty for http:// URLs, only for e.g. `data:` URLs")
        }
        (_, None) => unreachable!("{scheme} URLs always have a default port"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fromstr_https() {
        assert_eq!(
            "https://proxy-host:8000".parse::<ProxyUrl>().unwrap(),
            ProxyUrl {
                scheme: ProxyScheme::Https,
                host: url::Host::Domain("proxy-host".into()),
                port: Port(8000),
            }
        )
    }

    #[test]
    fn fromstr_https_port_80() {
        assert_eq!(
            "https://proxy-host:80".parse::<ProxyUrl>().unwrap(),
            ProxyUrl {
                scheme: ProxyScheme::Https,
                host: url::Host::Domain("proxy-host".into()),
                port: Port(80),
            }
        )
    }

    #[test]
    fn fromstr_http() {
        assert_eq!(
            "http://proxy-host:1234".parse::<ProxyUrl>().unwrap(),
            ProxyUrl {
                scheme: ProxyScheme::Http,
                host: url::Host::Domain("proxy-host".into()),
                port: Port(1234),
            }
        )
    }

    #[test]
    fn fromstr_with_ip() {
        assert_eq!(
            "http://192.168.1.2:1234".parse::<ProxyUrl>().unwrap(),
            ProxyUrl {
                scheme: ProxyScheme::Http,
                host: url::Host::Ipv4("192.168.1.2".parse().unwrap()),
                port: Port(1234),
            }
        )
    }

    #[test]
    fn fromstr_without_port() {
        assert_eq!(
            "http://192.168.1.2".parse::<ProxyUrl>().unwrap(),
            ProxyUrl {
                scheme: ProxyScheme::Http,
                host: url::Host::Ipv4("192.168.1.2".parse().unwrap()),
                port: Port(80),
            }
        )
    }

    #[test]
    fn fromstr_without_scheme() {
        assert_eq!(
            "192.168.1.2".parse::<ProxyUrl>().unwrap(),
            ProxyUrl {
                scheme: ProxyScheme::Http,
                host: url::Host::Ipv4("192.168.1.2".parse().unwrap()),
                port: Port(80),
            }
        )
    }

    #[test]
    fn fromstr_with_unknown_scheme() {
        assert_eq!(
            "nonsense://192.168.1.2"
                .parse::<ProxyUrl>()
                .unwrap_err()
                .to_string(),
            "Invalid proxy URL: Unsupported proxy scheme: nonsense"
        )
    }

    #[test]
    fn fromstr_with_unknown_scheme_and_invalid_port() {
        assert_eq!(
            "nonsense://192.168.1.2:notanumber"
                .parse::<ProxyUrl>()
                .unwrap_err()
                .to_string(),
            "Invalid proxy URL: invalid port number"
        )
    }

    #[test]
    fn fromstr_without_port_https() {
        assert_eq!(
            "https://192.168.1.2".parse::<ProxyUrl>().unwrap(),
            ProxyUrl {
                scheme: ProxyScheme::Https,
                host: url::Host::Ipv4("192.168.1.2".parse().unwrap()),
                port: Port(443),
            }
        )
    }

    #[test]
    fn fromstr_with_explicit_default_http_port() {
        assert_eq!(
            "http://192.168.1.2:80".parse::<ProxyUrl>().unwrap(),
            ProxyUrl {
                scheme: ProxyScheme::Http,
                host: url::Host::Ipv4("192.168.1.2".parse().unwrap()),
                port: Port(80),
            }
        )
    }

    #[test]
    fn fromstr_with_explicit_default_https_port() {
        assert_eq!(
            "https://192.168.1.2:443".parse::<ProxyUrl>().unwrap(),
            ProxyUrl {
                scheme: ProxyScheme::Https,
                host: url::Host::Ipv4("192.168.1.2".parse().unwrap()),
                port: Port(443),
            }
        )
    }

    #[test]
    fn fromstr_non_numeric_port() {
        assert_eq!(
            "http://192.168.1.2:test"
                .parse::<ProxyUrl>()
                .unwrap_err()
                .to_string(),
            "Invalid proxy URL: invalid port number"
        )
    }

    #[test]
    fn fromstr_without_host() {
        assert_eq!(
            "http://?test".parse::<ProxyUrl>().unwrap_err().to_string(),
            "Invalid proxy URL: empty host"
        )
    }

    #[test]
    fn fromstr_with_username() {
        assert_eq!(
            "http://username@host"
                .parse::<ProxyUrl>()
                .unwrap_err()
                .to_string(),
            "Invalid proxy URL: URL should not contain a username, please specify this in the dedicated configuration"
        )
    }

    #[test]
    fn fromstr_with_password() {
        assert_eq!(
            "http://:password@host"
                .parse::<ProxyUrl>()
                .unwrap_err()
                .to_string(),
            "Invalid proxy URL: URL should not contain a password, please specify this in the dedicated configuration"
        )
    }
}
