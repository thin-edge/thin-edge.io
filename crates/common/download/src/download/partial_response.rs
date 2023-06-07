//! Utilities related to parsing responses to partial requests.

use reqwest::header;
use reqwest::header::HeaderValue;
use reqwest::StatusCode;

/// Returns the position of the partial response in the resource.
///
/// When making a partial request, a server can return a different range than
/// the one we asked for, in which case we need to extract the position from the
/// Content-Range header. The server could also just ignore the Range header and
/// respond with 200 OK, in which case we need to download the entire resource
/// all over again.
pub fn response_range_start(response: &reqwest::Response) -> Result<u64, InvalidResponseError> {
    let chunk_pos = match response.status() {
        // Complete response, seek to the beginning of the file
        StatusCode::OK => 0,

        // Partial response, the range might be different from what we
        // requested, so we need to parse it. Because we only request a single
        // range from the current position to the end of the document, we can
        // ignore multipart/byteranges media type.
        StatusCode::PARTIAL_CONTENT => partial_response_start_range(response)?,

        // We don't expect to receive any other 200-299 status code, but if we
        // do, treat it the same as OK
        status_code if status_code.is_success() => 0,

        status_code => {
            return Err(InvalidResponseError::UnexpectedStatus(status_code));
        }
    };
    Ok(chunk_pos)
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
