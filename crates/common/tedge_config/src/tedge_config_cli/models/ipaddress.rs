use doku::Document;
use serde::Deserialize;
use serde::Serialize;
use std::convert::TryFrom;
use std::convert::TryInto;
use std::fmt;
use std::net::IpAddr;
use std::net::Ipv4Addr;

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Document)]
pub struct IpAddress(pub IpAddr);

#[derive(thiserror::Error, Debug)]
#[error("Invalid ip address: '{input}'.")]
pub struct InvalidIpAddress {
    input: String,
}

impl fmt::Display for IpAddress {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

impl Default for IpAddress {
    fn default() -> Self {
        IpAddress(IpAddr::V4(Ipv4Addr::LOCALHOST))
    }
}

impl TryFrom<&str> for IpAddress {
    type Error = InvalidIpAddress;

    fn try_from(input: &str) -> Result<Self, Self::Error> {
        input
            .parse::<IpAddr>()
            .map_err(|_| InvalidIpAddress {
                input: input.to_string(),
            })
            .map(IpAddress)
    }
}

impl TryFrom<String> for IpAddress {
    type Error = InvalidIpAddress;

    fn try_from(input: String) -> Result<Self, Self::Error> {
        input
            .parse::<IpAddr>()
            .map_err(|_| InvalidIpAddress { input })
            .map(IpAddress)
    }
}

impl TryInto<String> for IpAddress {
    type Error = std::convert::Infallible;

    fn try_into(self) -> Result<String, Self::Error> {
        Ok(self.to_string())
    }
}

impl From<IpAddress> for IpAddr {
    fn from(val: IpAddress) -> Self {
        val.0
    }
}

#[cfg(test)]
mod tests {
    use super::InvalidIpAddress;
    use super::IpAddress;
    use assert_matches::*;
    use std::net::IpAddr;
    use std::net::Ipv4Addr;
    use std::net::Ipv6Addr;

    #[test]
    fn conversion_from_valid_ipv4_succeeds() {
        let _loh: IpAddress = IpAddress::try_from("127.0.0.1").unwrap();
        assert_matches!(Ipv4Addr::LOCALHOST, _loh);
    }

    #[test]
    fn conversion_from_valid_ipv6_succeeds() {
        let _loh: IpAddress = IpAddress::try_from("::1").unwrap();
        assert_matches!(Ipv6Addr::LOCALHOST, _loh);
    }

    #[test]
    fn conversion_from_longer_integer_fails() {
        assert_matches!(IpAddress::try_from("66000"), Err(InvalidIpAddress { .. }));
    }

    #[test]
    fn conversion_from_ip_to_string() {
        assert_matches!(TryInto::<String>::try_into(IpAddress(IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1)))), Ok(ip_str) if ip_str == "::1");
    }
}
