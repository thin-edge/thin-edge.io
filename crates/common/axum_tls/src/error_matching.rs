use std::error::Error;

#[cfg(any(test, feature = "test-helpers"))]
pub fn assert_error_matches(err: &reqwest::Error, alert_description: rustls::AlertDescription) {
    assert_matches::assert_matches!(
        rustls_error_from_reqwest(err).unwrap(),
        rustls::Error::AlertReceived(des) if des == &alert_description
    );
}

pub fn rustls_error_from_reqwest(err: &reqwest::Error) -> Option<&rustls::Error> {
    err.source()?
        .downcast_ref::<hyper::Error>()?
        .source()?
        .downcast_ref::<std::io::Error>()?
        .get_ref()?
        .downcast_ref::<rustls::Error>()
}
