use csv::Writer;
use json_sm::SoftwareUpdateResponse;

#[derive(thiserror::Error, Debug)]
pub(crate) enum SmartRestSerializerError {
    #[error("The operation status is not executing. {response:?}")]
    StatusIsNotExecuting { response: SoftwareUpdateResponse },

    #[error("The operation status is not successful. {response:?}")]
    StatusIsNotSuccessful { response: SoftwareUpdateResponse },

    #[error("The operation status is not failed. {response:?}")]
    StatusIsNotFailed { response: SoftwareUpdateResponse },

    #[error("Failed to add double quotes to the error reason.")]
    DoubleQuoteError,

    #[error(transparent)]
    CsvError(#[from] csv::Error),

    #[error(transparent)]
    CsvWriterError(#[from] csv::IntoInnerError<Writer<Vec<u8>>>),

    #[error(transparent)]
    FromUtf8Error(#[from] std::string::FromUtf8Error),
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum SmartRestDeserializerError {
    #[error("The SmartREST message ID is not Update Software Operation (528).")]
    NotUpdateSoftwareOperation,

    #[error(transparent)]
    CsvError(#[from] csv::Error),
}
