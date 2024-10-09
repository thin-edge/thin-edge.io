use c8y_api::json_c8y::*;
use c8y_api::smartrest::error::SMCumulocityMapperError;
use std::collections::HashMap;
use std::path::PathBuf;
use tedge_actors::fan_in_message_type;
use tedge_actors::ChannelError;
use tedge_http_ext::HttpError;

fan_in_message_type!(C8YRestRequest[GetJwtToken, GetFreshJwtToken, CreateEvent, SoftwareListResponse, UploadLogBinary, UploadFile, DownloadFile]: Debug, PartialEq, Eq);
//HIPPO Rename EventId to String as there could be many other String responses as well and this macro doesn't allow another String variant
fan_in_message_type!(C8YRestResponse[EventId, Url, Unit]: Debug);

#[derive(thiserror::Error, Debug)]
pub enum C8YRestError {
    #[error(transparent)]
    FromChannel(#[from] ChannelError),

    // TODO impl a proper C8YRest Error type
    #[error(transparent)]
    FromC8YRest(#[from] SMCumulocityMapperError),

    #[error(transparent)]
    FromHttpError(#[from] HttpError),

    // FIXME: Consider to replace this error by a panic,
    //        since this can only happens if the actor is buggy
    //        e.g. responding to a request A with a response for B.
    #[error("Unexpected response")]
    ProtocolError,

    #[error("Failed with {0}")]
    CustomError(String),

    #[error(transparent)]
    FromDownloadError(#[from] download::DownloadError),

    #[error(transparent)]
    FromFileError(#[from] tedge_utils::file::FileError),

    #[error(transparent)]
    FromIOError(#[from] std::io::Error),

    // `Display` impl of `C8yRestError` is used as part of an error message sent to the cloud in a smartrest message.
    // Using `{anyhow::Error:?}` also prints the lower-level cause, so using it here will result in a more detailed
    // error message being sent to the cloud
    #[error("Unexpected error: {0:?}")]
    Other(#[from] anyhow::Error),
}

pub type C8YRestResult = Result<C8YRestResponse, C8YRestError>;

#[derive(Debug, PartialEq, Eq)]
pub struct GetJwtToken;

#[derive(Debug, PartialEq, Eq)]
pub struct GetFreshJwtToken;

#[derive(Debug, PartialEq, Eq)]
pub struct CreateEvent {
    pub event_type: String,
    pub time: time::OffsetDateTime,
    pub text: String,
    pub extras: HashMap<String, serde_json::Value>,
    /// C8y's external ID of the device
    pub device_id: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct SoftwareListResponse {
    pub c8y_software_list: C8yUpdateSoftwareListResponse,
    pub device_id: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct UploadLogBinary {
    pub log_type: String,
    pub log_content: String,
    pub device_id: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct UploadFile {
    pub file_path: PathBuf,
    pub file_type: String,
    pub device_id: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DownloadFile {
    pub download_url: String,
    pub file_path: PathBuf,
}

pub type EventId = String;

pub type Unit = ();

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Url(pub String);

// Transform any unexpected message into an error
impl From<C8YRestResult> for C8YRestError {
    fn from(result: C8YRestResult) -> Self {
        match result {
            Err(rest_err) => rest_err,
            _ => C8YRestError::ProtocolError,
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum C8YConnectionError {
    #[error("The connection has been interrupted before the internal id has been retrieved")]
    Interrupted,
}
