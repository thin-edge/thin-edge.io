//! Detecting requests that tedge-p11-server served from a different token or key than requested.
//!
//! tedge-p11-server uses the `device.cryptoki.uri` it was started with as a base for every request
//! URI, and when both set the same attribute (e.g. `token`), the server's value wins. A request for
//! one token can therefore be silently served from another. The server doesn't report this, so the
//! commands detect it by comparing the URIs of the objects returned by the server with the
//! requested URI.
//!
//! This only works for objects the server reports back. In particular, `list_keys` lists every key
//! on the token the server selected, ignoring the `object`/`id` attributes, so a pre-check can find
//! out that another token was selected, but not that another key would be.

use anyhow::Context;
use tedge_p11::pkcs11::uri::Pkcs11Uri;
use tedge_p11::service::ListKeysRequest;
use tedge_p11::service::TedgeP11Service;
use tedge_p11::SecretString;

use crate::warning;

/// Tells the user how to make tedge-p11-server use the URIs they request.
pub const UNSET_CONFIG_URI_HINT: &str =
    "If you want `tedge hsm` commands to access all tokens and \
     keys, unset `device.cryptoki.uri` for tedge-p11-server and restart it.";

/// Returns true if the object identified by `uri` matches `filter`.
///
/// A `uri` that can't be parsed doesn't match.
pub fn matches_uri(filter: &Pkcs11Uri, uri: &str) -> bool {
    Pkcs11Uri::parse(uri).is_ok_and(|uri| filter.matches(&uri))
}

/// Returns a filter matching any object on the token(s) selected by `uri`.
pub fn token_filter<'a>(uri: &Pkcs11Uri<'a>) -> Pkcs11Uri<'a> {
    Pkcs11Uri {
        token: uri.token.clone(),
        serial: uri.serial.clone(),
        slot_id: uri.slot_id,
        ..Default::default()
    }
}

/// Warns that `count` objects returned by tedge-p11-server were not shown because they don't match
/// the requested URI.
pub fn warn_filtered(count: usize) {
    warning!(
        "{count} results were not shown because tedge-p11-server filters objects using `device.cryptoki.uri` option.\n    {UNSET_CONFIG_URI_HINT}\n"
    );
}

/// Key objects found on a token other than the requested one.
#[derive(Debug)]
pub struct OtherTokenSelected {
    /// URI of one of the key objects, identifying the token the server selected.
    pub key_uri: String,
    /// The number of key objects on the selected token.
    pub objects: usize,
}

impl OtherTokenSelected {
    /// An error refusing to `operation` because the server selected another token.
    pub fn into_error(self, operation: &str) -> anyhow::Error {
        anyhow::anyhow!(
            "Refusing to {operation}: tedge-p11-server selected a different token than the \
             requested one, because `device.cryptoki.uri` restricts the tokens it can use. {} key \
             object(s) on that token would be affected, e.g. '{}'.\n{UNSET_CONFIG_URI_HINT}",
            self.objects,
            self.key_uri,
        )
    }
}

/// Finds out whether tedge-p11-server would serve a request for `uri` from another token.
///
/// The keys listed for `uri` are on the token the server selects for the request. Returns `None`
/// if they are all on the requested token, or if there are no keys, in which case the selected
/// token can't be identified.
pub fn other_token_selected(
    cryptoki: &dyn TedgeP11Service,
    uri: &str,
    pin: Option<SecretString>,
) -> anyhow::Result<Option<OtherTokenSelected>> {
    let requested = Pkcs11Uri::parse(uri).context("Failed to parse PKCS #11 URI")?;
    let filter = token_filter(&requested);

    let response = cryptoki.list_keys(ListKeysRequest {
        uri: Some(uri.to_string()),
        pin,
    })?;

    let objects = response.keys.len();
    let other_token_key = response
        .keys
        .into_iter()
        .find(|key| !matches_uri(&filter, &key.uri));

    Ok(other_token_key.map(|key| OtherTokenSelected {
        key_uri: key.uri,
        objects,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tedge_p11::service::KeyDetails;
    use tedge_p11::service::ListKeysResponse;

    /// A [`TedgeP11Service`] stub where only [`list_keys`] works, returning keys with the given URIs.
    ///
    /// [`list_keys`]: TedgeP11Service::list_keys
    struct FakeCryptoki {
        key_uris: Vec<&'static str>,
    }

    impl TedgeP11Service for FakeCryptoki {
        fn list_keys(&self, _: ListKeysRequest) -> anyhow::Result<ListKeysResponse> {
            let keys = self
                .key_uris
                .iter()
                .map(|uri| KeyDetails {
                    class: "private".to_string(),
                    key_type: "EC".to_string(),
                    label: String::new(),
                    id: String::new(),
                    uri: uri.to_string(),
                })
                .collect();
            Ok(ListKeysResponse { keys })
        }

        fn choose_scheme(
            &self,
            _: tedge_p11::service::ChooseSchemeRequest,
        ) -> anyhow::Result<tedge_p11::service::ChooseSchemeResponse> {
            unimplemented!()
        }

        fn sign(
            &self,
            _: tedge_p11::service::SignRequestWithSigScheme,
        ) -> anyhow::Result<tedge_p11::service::SignResponse> {
            unimplemented!()
        }

        fn get_public_key_pem(&self, _: Option<&str>) -> anyhow::Result<String> {
            unimplemented!()
        }

        fn get_tokens_uris(&self) -> anyhow::Result<Vec<String>> {
            unimplemented!()
        }

        fn list_tokens(&self) -> anyhow::Result<tedge_p11::service::ListTokensResponse> {
            unimplemented!()
        }

        fn change_pin(
            &self,
            _: tedge_p11::service::ChangePinRequest,
        ) -> anyhow::Result<tedge_p11::service::ChangePinResponse> {
            unimplemented!()
        }

        fn delete_key(
            &self,
            _: tedge_p11::service::DeleteKeyRequest,
        ) -> anyhow::Result<tedge_p11::service::DeleteKeyResponse> {
            unimplemented!()
        }

        fn create_key(
            &self,
            _: tedge_p11::service::CreateKeyRequest,
        ) -> anyhow::Result<tedge_p11::service::CreateKeyResponse> {
            unimplemented!()
        }

        fn init_token(
            &self,
            _: tedge_p11::service::InitTokenRequest,
        ) -> anyhow::Result<tedge_p11::service::InitTokenResponse> {
            unimplemented!()
        }
    }

    #[test]
    fn token_filter_ignores_object_attributes() {
        let uri = Pkcs11Uri::parse("pkcs11:token=t1;serial=s1;object=k1;id=%01").unwrap();
        let filter = token_filter(&uri);
        assert!(matches_uri(
            &filter,
            "pkcs11:token=t1;serial=s1;object=k2;id=%02"
        ));
        assert!(!matches_uri(
            &filter,
            "pkcs11:token=t2;serial=s1;object=k1;id=%01"
        ));
    }

    #[test]
    fn unparseable_uri_does_not_match() {
        let filter = Pkcs11Uri::parse("pkcs11:").unwrap();
        assert!(!matches_uri(&filter, "<unknown>"));
    }

    #[test]
    fn requested_token_selected() {
        let cryptoki = FakeCryptoki {
            key_uris: vec![
                "pkcs11:serial=s1;token=t1;id=%01;object=k1",
                "pkcs11:serial=s1;token=t1;id=%02;object=k2",
            ],
        };
        let result = other_token_selected(&cryptoki, "pkcs11:token=t1;object=k3", None).unwrap();
        assert!(result.is_none(), "{result:?}");
    }

    #[test]
    fn other_token_selected_is_detected() {
        let cryptoki = FakeCryptoki {
            key_uris: vec![
                "pkcs11:serial=s2;token=t2;id=%01;object=k1",
                "pkcs11:serial=s2;token=t2;id=%01;object=k1",
            ],
        };
        let result = other_token_selected(&cryptoki, "pkcs11:token=t1", None)
            .unwrap()
            .unwrap();
        assert_eq!(result.objects, 2);
        assert_eq!(result.key_uri, "pkcs11:serial=s2;token=t2;id=%01;object=k1");
    }

    #[test]
    fn selected_token_is_unknown_without_keys() {
        let cryptoki = FakeCryptoki { key_uris: vec![] };
        let result = other_token_selected(&cryptoki, "pkcs11:token=t1", None).unwrap();
        assert!(result.is_none(), "{result:?}");
    }
}
