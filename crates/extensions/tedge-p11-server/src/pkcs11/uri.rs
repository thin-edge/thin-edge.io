use std::borrow::Cow;
use std::collections::HashMap;

/// Attributes decoded from a PKCS #11 URL.
///
/// Attributes only relevant to us shall be put into fields and the rest is in `other` hashmap.
///
/// https://www.rfc-editor.org/rfc/rfc7512.html
#[derive(Debug, Clone, Default)]
pub struct Pkcs11Uri<'a> {
    pub token: Option<Cow<'a, str>>,
    pub serial: Option<Cow<'a, str>>,
    pub id: Option<Vec<u8>>,
    pub object: Option<Cow<'a, str>>,
    pub other: HashMap<&'a str, Cow<'a, str>>,
}

impl<'a> Pkcs11Uri<'a> {
    pub fn parse(uri: &'a str) -> anyhow::Result<Self> {
        let path = uri
            .strip_prefix("pkcs11:")
            .ok_or_else(|| anyhow::anyhow!("missing PKCS #11 URI scheme"))?;

        // split of the query component
        let path = path.split_once('?').map(|(l, _)| l).unwrap_or(path);

        // parse attributes, duplicate attributes are an error (RFC section 2.3)
        let pairs_iter = path.split(';').filter_map(|pair| pair.split_once('='));
        let mut pairs: HashMap<&str, &str> = HashMap::new();
        for (k, v) in pairs_iter {
            let prev_value = pairs.insert(k, v);
            if prev_value.is_some() {
                anyhow::bail!("PKCS#11 URI contains duplicate attribute ({k})");
            }
        }

        let token = pairs
            .remove("token")
            .map(|v| percent_encoding::percent_decode_str(v).decode_utf8_lossy());
        let serial = pairs
            .remove("serial")
            .map(|v| percent_encoding::percent_decode_str(v).decode_utf8_lossy());
        let object = pairs
            .remove("object")
            .map(|v| percent_encoding::percent_decode_str(v).decode_utf8_lossy());

        let id: Option<Vec<u8>> = pairs
            .remove("id")
            .map(|id| percent_encoding::percent_decode_str(id).collect());

        let other = pairs
            .into_iter()
            .map(|(k, v)| {
                (
                    k,
                    percent_encoding::percent_decode_str(v).decode_utf8_lossy(),
                )
            })
            .collect();

        Ok(Self {
            token,
            serial,
            id,
            object,
            other,
        })
    }

    /// Add new attributes from `other` to `self`.
    ///
    /// If other contains new attributes not present in self, add them to self. If these
    /// attributes are already present in self, preserve value currently in self.
    pub fn append_attributes(&mut self, other: Self) {
        self.token = self.token.take().or(other.token);
        self.serial = self.serial.take().or(other.serial);
        self.id = self.id.take().or(other.id);
        self.object = self.object.take().or(other.object);

        for (attribute, value) in other.other {
            if !self.other.contains_key(attribute) {
                self.other.insert(attribute, value);
            }
        }
    }
}

const PKCS11_ASCII_SET: &percent_encoding::AsciiSet =
    &percent_encoding::NON_ALPHANUMERIC.remove(b'-');

/// Percent-encode PKCS11 attribute values.
///
/// In contrast to more general URL percent-encoding, some characters like `-` don't need to be
/// percent-encoded in PKCS11 URIs[1], so we don't encode them. Note that if we did, encoding these
/// characters that don't have to be encoded is not a mistake, as any URI parser would eagerly
/// decode all percent-encode sequences, the difference is just better/worse readability for the
/// user.
///
/// [1]: https://www.rfc-editor.org/rfc/rfc7512.html#section-2.3
pub fn percent_encode(input: &str) -> String {
    percent_encoding::utf8_percent_encode(input, PKCS11_ASCII_SET).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_valid_pkcs11_uri() {
        // test input URIs taken from RFC examples section and combined with properties we actually use
        // https://www.rfc-editor.org/rfc/rfc7512.html#section-3
        let input = "pkcs11:token=The%20Software%20PKCS%2311%20Softtoken;\
            manufacturer=Snake%20Oil,%20Inc.;\
            model=1.0;\
            object=my-certificate;\
            type=cert;\
            id=%69%95%3E%5C%F4%BD%EC%91;\
            serial=\
            ?pin-source=file:/etc/token_pin";

        let attributes = Pkcs11Uri::parse(input).unwrap();

        assert_eq!(attributes.token.unwrap(), "The Software PKCS#11 Softtoken");
        assert_eq!(
            attributes.other.get("manufacturer").unwrap(),
            "Snake Oil, Inc."
        );
        assert_eq!(attributes.other.get("model").unwrap(), "1.0");
        assert_eq!(attributes.serial.unwrap(), "");
        assert_eq!(attributes.object.unwrap(), "my-certificate");
        assert_eq!(
            attributes.id,
            Some(vec![0x69, 0x95, 0x3e, 0x5c, 0xf4, 0xbd, 0xec, 0x91])
        );
    }

    #[test]
    fn fails_on_uris_with_duplicate_attributes() {
        let input = "pkcs11:token=my-token;token=my-token";
        let err = Pkcs11Uri::parse(input).unwrap_err();
        assert!(err
            .to_string()
            .contains("PKCS#11 URI contains duplicate attribute (token)"));
    }

    #[test]
    fn fails_on_uris_with_invalid_scheme() {
        let input = "not a pkcs#11 uri";
        let err = Pkcs11Uri::parse(input).unwrap_err();
        assert!(err.to_string().contains("missing PKCS #11 URI scheme"));
    }

    #[test]
    fn appends_attributes_correctly() {
        let mut uri1 = Pkcs11Uri::parse("pkcs11:token=token1").unwrap();
        let uri2 = Pkcs11Uri::parse(
            "pkcs11:token=token2;serial=serial2;id=%01%02;object=object2;key1=value1",
        )
        .unwrap();

        uri1.append_attributes(uri2);

        assert_eq!(uri1.token.unwrap(), "token1");
        assert_eq!(uri1.serial.unwrap(), "serial2");
        assert_eq!(uri1.id, Some(vec![0x01, 0x02]));
        assert_eq!(uri1.object.unwrap(), "object2");
        assert_eq!(uri1.other.get("key1").unwrap(), "value1");
    }

    #[test]
    fn appends_attributes_with_no_conflicts() {
        let mut uri1 = Pkcs11Uri::parse("pkcs11:").unwrap();
        let uri2 = Pkcs11Uri::parse(
            "pkcs11:token=token2;serial=serial2;id=%01%02;object=object2;key1=value1",
        )
        .unwrap();

        uri1.append_attributes(uri2);

        assert_eq!(uri1.token.unwrap(), "token2");
        assert_eq!(uri1.serial.unwrap(), "serial2");
        assert_eq!(uri1.id, Some(vec![0x01, 0x02]));
        assert_eq!(uri1.object.unwrap(), "object2");
        assert_eq!(uri1.other.get("key1").unwrap(), "value1");
    }

    #[test]
    fn does_not_override_existing_attributes() {
        let mut uri1 = Pkcs11Uri::parse(
            "pkcs11:token=token1;serial=serial1;id=%01;object=object1;key1=value1",
        )
        .unwrap();
        let uri2 = Pkcs11Uri::parse(
            "pkcs11:token=token2;serial=serial2;id=%02;object=object2;key2=value2",
        )
        .unwrap();

        uri1.append_attributes(uri2);

        assert_eq!(uri1.token.unwrap(), "token1");
        assert_eq!(uri1.serial.unwrap(), "serial1");
        assert_eq!(uri1.id, Some(vec![0x01]));
        assert_eq!(uri1.object.unwrap(), "object1");
        assert_eq!(uri1.other.get("key1").unwrap(), "value1");
        assert_eq!(uri1.other.get("key2").unwrap(), "value2");
    }
}
