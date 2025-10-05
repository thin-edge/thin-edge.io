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
        if let Some(rem) = s.strip_prefix("https://") {
            let (host, port) = parse_host_port(rem)?;
            Ok(Self {
                scheme: ProxyScheme::Https,
                host,
                port,
            })
        } else if let Some(rem) = s.strip_prefix("http://") {
            let (host, port) = parse_host_port(rem)?;
            Ok(Self {
                scheme: ProxyScheme::Http,
                host,
                port,
            })
        } else {
            let (host, port) = parse_host_port(s)?;
            Ok(Self {
                scheme: ProxyScheme::Http,
                host,
                port,
            })
        }
    }
}

fn parse_host_port(s: &str) -> anyhow::Result<(url::Host, Port)> {
    let url = url::Url::parse(&format!("http://{s}"))?;
    ensure!(
        url.username() == "",
        "URL should not contain a username, please specify this in the dedicated configuration"
    );
    ensure!(
        url.password().is_none(),
        "URL should not contain a password, please specify this in the dedicated configuration"
    );
    match (url.host(), url.port_or_known_default()) {
        (Some(host), Some(port)) => Ok((host.to_owned(), Port(port))),
        (None, _) => {
            unreachable!("Host cannot be empty for http:// URLs, only for e.g. `data:` URLs")
        }
        (_, None) => bail!("{s} is missing a port"),
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
