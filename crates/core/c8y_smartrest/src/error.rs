use sm_interface::SoftwareUpdateResponse;

#[derive(thiserror::Error, Debug)]
pub enum SmartRestSerializerError {
    #[error("The operation status is not supported. {response:?}")]
    UnsupportedOperationStatus { response: SoftwareUpdateResponse },

    #[error("Failed to serialize SmartREST.")]
    InvalidCsv(#[from] csv::Error),

    #[error(transparent)]
    FromCsvWriter(#[from] csv::IntoInnerError<csv::Writer<Vec<u8>>>),

    #[error(transparent)]
    FromUtf8Error(#[from] std::string::FromUtf8Error),

    #[error(transparent)]
    FromTimeFormatError(#[from] time::error::Format),
}

#[derive(thiserror::Error, Debug)]
pub enum SmartRestDeserializerError {
    #[error("The received SmartREST message ID {id} is unsupported.")]
    UnsupportedOperation { id: String },

    #[error("Failed to deserialize SmartREST.")]
    InvalidCsv(#[from] csv::Error),

    #[error("Jwt response contains incorrect ID: {0}")]
    InvalidMessageId(u16),

    #[error("Parameter {parameter} is not recognized. {hint}")]
    InvalidParameter {
        operation: String,
        parameter: String,
        hint: String,
    },

    #[error("Empty request")]
    EmptyRequest,
}
