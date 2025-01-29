use std::error::Error;

#[cfg(any(test, feature = "test-helpers"))]
pub fn assert_error_matches(err: reqwest::Error, alert_description: rustls::AlertDescription) {
    let rustls_err = match rustls_error_from_reqwest(&err) {
        Some(err) => err,
        None => panic!("{:?}", anyhow::Error::from(err)),
    };
    assert_matches::assert_matches!(
        rustls_err,
        rustls::Error::AlertReceived(des) if des == &alert_description
    );
}

pub fn rustls_error_from_reqwest(err: &reqwest::Error) -> Option<&rustls::Error> {
    err.source()?
        .downcast_ref::<hyper_util::client::legacy::Error>()?
        .source()?
        .source()?
        .downcast_ref::<std::io::Error>()?
        .get_ref()?
        .downcast_ref::<rustls::Error>()
}
