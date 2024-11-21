use c8y_api::json_c8y::*;
use std::collections::HashMap;
use tedge_actors::ChannelError;
use tedge_http_ext::HttpError;

#[derive(thiserror::Error, Debug)]
pub enum C8YRestError {
    #[error(transparent)]
    FromChannel(#[from] ChannelError),

    #[error(transparent)]
    FromHttpError(#[from] HttpError),

    #[error("Failed with {0}")]
    CustomError(String),

    // `Display` impl of `C8yRestError` is used as part of an error message sent to the cloud in a smartrest message.
    // Using `{anyhow::Error:?}` also prints the lower-level cause, so using it here will result in a more detailed
    // error message being sent to the cloud
    #[error("Unexpected error: {0:?}")]
    Other(#[from] anyhow::Error),

    #[error(transparent)]
    InitConnectionFailed(#[from] C8YConnectionError),
}

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
pub(crate) struct SoftwareListResponse {
    pub c8y_software_list: C8yUpdateSoftwareListResponse,
    pub device_id: String,
}

pub type EventId = String;

#[derive(thiserror::Error, Debug)]
pub enum C8YConnectionError {
    #[error("The connection has been interrupted before the internal id has been retrieved")]
    Interrupted,
}
