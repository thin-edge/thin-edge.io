use c8y_api::json_c8y::*;
use c8y_api::smartrest::error::SMCumulocityMapperError;
use std::path::PathBuf;
use tedge_actors::fan_in_message_type;
use tedge_actors::ChannelError;
use tedge_http_ext::HttpError;
use tedge_utils::file::PermissionEntry;

fan_in_message_type!(C8YRestRequest[C8yCreateEvent, C8yUpdateSoftwareListResponse, UploadLogBinary, UploadConfigFile, DownloadFile]: Debug);
fan_in_message_type!(C8YRestResponse[EventId, Unit]: Debug);

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

    #[error("JWT token could not be retrieved")]
    JWTTokenError,

    #[error("Failed with {0}")]
    CustomError(String),
}

pub type C8YRestResult = Result<C8YRestResponse, C8YRestError>;

#[derive(Debug)]
pub struct UploadLogBinary {
    pub log_type: String,
    pub log_content: String,
    pub child_device_id: Option<String>,
}

#[derive(Debug)]
pub struct UploadConfigFile {
    pub config_path: PathBuf,
    pub config_type: String,
    pub child_device_id: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DownloadFile {
    pub download_url: String,
    pub file_path: PathBuf,
    pub file_permissions: PermissionEntry,
}

pub type EventId = String;

pub type Unit = ();

// Transform any unexpected message into an error
impl From<Option<C8YRestResult>> for C8YRestError {
    fn from(maybe_result: Option<C8YRestResult>) -> Self {
        match maybe_result {
            None => ChannelError::ReceiveError().into(),
            Some(Err(rest_err)) => rest_err,
            _ => C8YRestError::ProtocolError,
        }
    }
}
