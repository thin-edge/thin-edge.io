//! Utilities related to parsing responses to partial requests.

use reqwest::header;
use reqwest::header::HeaderValue;
use reqwest::Response;
use reqwest::StatusCode;

pub(super) enum PartialResponse {
    /// Server returned a partial content response starting at given position
    PartialContent(u64),

    /// Server returned regular OK response, resume writing at 0
    CompleteContent,

    /// Server returned partial content but resource was modified, request needs to be retried
    ResourceModified,
}

/// Returns the position of the partial response in the resource.
///
/// When making a partial request, a server can return a different range than
/// the one we asked for, in which case we need to extract the position from the
/// Content-Range header. The server could also just ignore the Range header and
/// respond with 200 OK, in which case we need to download the entire resource
/// all over again.
pub(super) fn response_range_start(
    response: &reqwest::Response,
    prev_response: &Response,
) -> Result<PartialResponse, InvalidResponseError> {
    match response.status() {
        // Complete response, seek to the beginning of the file
        StatusCode::OK => Ok(PartialResponse::CompleteContent),

        // Partial response, the range might be different from what we
        // requested, so we need to parse it. Because we only request a single
        // range from the current position to the end of the document, we can
        // ignore multipart/byteranges media type.
        StatusCode::PARTIAL_CONTENT => {
            if was_resource_modified(response, prev_response) {
                return Ok(PartialResponse::ResourceModified);
            }
            let pos = partial_response_start_range(response)?;
            Ok(PartialResponse::PartialContent(pos))
        }

        // We don't expect to receive any other 200-299 status code, but if we
        // do, treat it the same as OK
        status_code if status_code.is_success() => Ok(PartialResponse::CompleteContent),

        status_code => Err(InvalidResponseError::UnexpectedStatus(status_code)),
    }
}

/// Checks if the resource was modified between the current and previous response.
///
/// If the resource was updated, we should restart download and request full range of the new
/// resource. Otherwise, a partial request can be used to resume the download.
fn was_resource_modified(response: &Response, prev_response: &Response) -> bool {
    if response.status() != hyper::StatusCode::PARTIAL_CONTENT {
        // not using a partial request, don't care if it's modified or not
        return false;
    }

    // etags in current and previous request must match
    let etag = response
        .headers()
        .get(header::ETAG)
        .and_then(|h| h.to_str().ok());
    let prev_etag = prev_response
        .headers()
        .get(header::ETAG)
        .and_then(|h| h.to_str().ok());

    match (etag, prev_etag) {
        (None, None) => {
            // no etags in either request, assume resource is unchanged
            false
        }
        (None, Some(_)) | (Some(_), None) => {
            // previous request didn't have etag and this does or vice versa, abort
            true
        }
        (Some(etag), Some(prev_etag)) => {
            // Examples:
            // ETag: "xyzzy"
            // ETag: W/"xyzzy"
            // ETag: ""
            if etag.starts_with("W/") {
                // validator is weak, but in range requests tags must match using strong comparison
                // https://www.rfc-editor.org/rfc/rfc9110#entity.tag.comparison
                return true;
            }

            etag != prev_etag
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum InvalidResponseError {
    #[error("Received unexpected status code: {0}, {reason:?}", reason = .0.canonical_reason())]
    UnexpectedStatus(StatusCode),
    #[error(transparent)]
    ContentRange(#[from] ContentRangeParseError),
}

/// Extracts the start of the range from the HTTP Content-Range header.
fn partial_response_start_range(
    response: &reqwest::Response,
) -> Result<u64, ContentRangeParseError> {
    let header_value =
        response
            .headers()
            .get(header::CONTENT_RANGE)
            .ok_or(ContentRangeParseError {
                reason: "Response was Partial Content but Content-Range header is missing",
                value: HeaderValue::from_str("").unwrap(),
            })?;

    let (unit, range) = header_value
        .to_str()
        .map_err(|_| ContentRangeParseError {
            reason: "Not valid utf-8",
            value: header_value.clone(),
        })
        .and_then(|value| {
            value.split_once(' ').ok_or(ContentRangeParseError {
                reason: "Invalid value in Content-Range header",
                value: header_value.clone(),
            })
        })?;
    if unit != "bytes" {
        return Err(ContentRangeParseError {
            reason: "unknown unit",
            value: header_value.clone(),
        });
    }
    let (range_start, _) = range.split_once('-').ok_or(ContentRangeParseError {
        reason: "invalid range",
        value: header_value.clone(),
    })?;
    range_start.parse().map_err(|_| ContentRangeParseError {
        reason: "failed to parse int",
        value: header_value.clone(),
    })
}

#[derive(Debug, thiserror::Error)]
#[error("Error parsing Content-Range header, reason: {reason}, got: {value:?}")]
pub struct ContentRangeParseError {
    reason: &'static str,
    value: header::HeaderValue,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case::test_case(Some(r#""xyzzy""#), Some(r#""xyzzy""#), false)]
    #[test_case::test_case(Some(r#"W/"xyzzy""#), Some(r#""xyzzy""#), true)]
    #[test_case::test_case(Some(r#""xyzzy""#), Some(r#"W/"xyzzy""#), true)]
    #[test_case::test_case(Some(r#""xyzzy1""#), Some(r#""xyzzy2""#), true)]
    #[test_case::test_case(None, None, false)]
    #[test_case::test_case(Some(r#""xyzzy1""#), None, true)]
    #[test_case::test_case(None, Some(r#""xyzzy2""#), true)]
    fn verifies_etags(etag1: Option<&'static str>, etag2: Option<&'static str>, modified: bool) {
        let mut response1 = http::Response::builder().status(StatusCode::PARTIAL_CONTENT);
        if let Some(etag) = etag1 {
            response1 = response1.header(http::header::ETAG, etag);
        }
        let response1 = response1.body("").unwrap().into();

        let mut response2 = http::Response::builder().status(StatusCode::PARTIAL_CONTENT);
        if let Some(etag) = etag2 {
            response2 = response2.header(http::header::ETAG, etag);
        }
        let response2 = response2.body("").unwrap().into();

        assert_eq!(was_resource_modified(&response1, &response2), modified);
    }
}
