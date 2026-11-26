//! Helpers for `tedge hsm` commands that list objects, which tedge-p11-server may filter using its
//! `device.cryptoki.uri`.
//!
//! For read-only requests the attributes of `device.cryptoki.uri` win over the requested ones, so
//! the server can return objects that don't match the request. Commands that create or delete
//! objects don't need such checks, as the server rejects requests conflicting with its
//! `device.cryptoki.uri`.

use tedge_p11::pkcs11::uri::Pkcs11Uri;

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

/// Warns that `count` objects returned by tedge-p11-server were not shown because they don't match
/// the requested URI.
pub fn warn_filtered(count: usize) {
    warning!(
        "{count} results were not shown because tedge-p11-server filters objects using `device.cryptoki.uri` option.\n    {UNSET_CONFIG_URI_HINT}\n"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unparseable_uri_does_not_match() {
        let filter = Pkcs11Uri::parse("pkcs11:").unwrap();
        assert!(!matches_uri(&filter, "<unknown>"));
    }
}
