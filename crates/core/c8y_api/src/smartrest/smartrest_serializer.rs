use crate::smartrest::csv::fields_to_csv_string;
use crate::smartrest::error::SmartRestSerializerError;
use crate::smartrest::topic::C8yTopic;
use csv::StringRecord;
use mqtt_channel::Message;
use serde::ser::SerializeSeq;
use serde::Deserialize;
use serde::Serialize;
use serde::Serializer;
use tracing::warn;

pub type SmartRest = String;

pub fn request_pending_operations() -> &'static str {
    "500"
}

/// Generates a SmartREST message to set the provided operation to executing
pub fn set_operation_executing(operation: impl C8yOperation) -> String {
    fields_to_csv_string(&["501", operation.name()])
}

/// Generates a SmartREST message to set the provided operation to failed with the provided reason
pub fn fail_operation(operation: impl C8yOperation, reason: &str) -> String {
    // If the failure reason exceeds 500 bytes, trancuate it
    if reason.len() <= 500 {
        fields_to_csv_string(&["502", operation.name(), reason])
    } else {
        warn!("Failure reason too long, message trancuated to 500 bytes");
        fields_to_csv_string(&["502", operation.name(), &reason[..500]])
    }
}

/// Generates a SmartREST message to set the provided operation to successful without a payload
pub fn succeed_operation_no_payload(operation: CumulocitySupportedOperations) -> String {
    succeed_static_operation(operation, None::<&str>)
}

/// Generates a SmartREST message to set the provided operation to successful with an optional payload
pub fn succeed_static_operation(
    operation: CumulocitySupportedOperations,
    payload: Option<impl AsRef<str>>,
) -> String {
    let mut wtr = csv::Writer::from_writer(vec![]);
    // Serialization will never fail for text
    match payload {
        Some(payload) => wtr.serialize(("503", operation.name(), payload.as_ref())),
        None => wtr.serialize(("503", operation.name())),
    }
    .unwrap();
    let mut output = wtr.into_inner().unwrap();
    output.pop();
    String::from_utf8(output).unwrap()
}

/// Generates a SmartREST message to set the provided custom operation to successful with a text or csv payload
///
/// - If the payload is "text", then a single payload field will be created
/// - If the payload is "csv", then the provided CSV record will be appended to the SmartREST message
///
/// # CSV
/// If the provided CSV does not match the standard Cumulocity format, the standard CSV escaping
/// rules will be applied. For example, `a,field "with" quotes` will be converted to
/// `a,"field ""with"" quotes"` before being appended to the output of this function.
///
/// # Errors
/// This will return an error if the payload is a CSV with multiple records, or an empty CSV.
pub fn succeed_operation(
    operation: &str,
    reason: impl Into<TextOrCsv>,
) -> Result<String, SmartRestSerializerError> {
    let mut wtr = csv::Writer::from_writer(vec![]);
    // Serialization can fail for CSV, but not for text
    wtr.serialize(("503", operation, reason.into()))?;
    let mut output = wtr.into_inner().unwrap();
    output.pop();
    Ok(String::from_utf8(output)?)
}

#[derive(Debug, Copy, Clone)]
pub enum CumulocitySupportedOperations {
    C8ySoftwareUpdate,
    C8yLogFileRequest,
    C8yRestartRequest,
    C8yUploadConfigFile,
    C8yDownloadConfigFile,
    C8yFirmware,
}

impl From<CumulocitySupportedOperations> for &'static str {
    fn from(op: CumulocitySupportedOperations) -> Self {
        match op {
            CumulocitySupportedOperations::C8ySoftwareUpdate => "c8y_SoftwareUpdate",
            CumulocitySupportedOperations::C8yLogFileRequest => "c8y_LogfileRequest",
            CumulocitySupportedOperations::C8yRestartRequest => "c8y_Restart",
            CumulocitySupportedOperations::C8yUploadConfigFile => "c8y_UploadConfigFile",
            CumulocitySupportedOperations::C8yDownloadConfigFile => "c8y_DownloadConfigFile",
            CumulocitySupportedOperations::C8yFirmware => "c8y_Firmware",
        }
    }
}

pub fn declare_supported_operations(ops: &[&str]) -> String {
    format!("114,{}", fields_to_csv_string(ops))
}

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct SmartRestSoftwareModuleItem {
    pub software: String,
    pub version: Option<String>,
    pub url: Option<String>,
}

/// A supported operation of the thin-edge device, used in status updates via SmartREST
///
/// This has two implementations, `&str` for custom operations, and [CumolocitySupportedOperations]
/// for statically supported operations.
pub trait C8yOperation {
    fn name(&self) -> &str;
}

impl C8yOperation for CumulocitySupportedOperations {
    fn name(&self) -> &str {
        (*self).into()
    }
}

impl<'a> C8yOperation for &'a str {
    fn name(&self) -> &str {
        self
    }
}

impl<'a> C8yOperation for &'a String {
    fn name(&self) -> &str {
        self.as_str()
    }
}

#[derive(Debug, Serialize, Eq, PartialEq)]
#[serde(untagged)]
pub enum TextOrCsv {
    Text(String),
    Csv(EmbeddedCsv),
}

impl From<EmbeddedCsv> for TextOrCsv {
    fn from(value: EmbeddedCsv) -> Self {
        Self::Csv(value)
    }
}

impl From<String> for TextOrCsv {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct EmbeddedCsv(String);

impl EmbeddedCsv {
    pub fn new(value: String) -> Self {
        Self(value)
    }
}

impl Serialize for EmbeddedCsv {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        S::Error: serde::ser::Error,
    {
        let record = parse_single_record(&self.0)?;
        let mut seq = serializer.serialize_seq(Some(record.len()))?;
        for field in record.iter() {
            seq.serialize_element(field)?;
        }
        seq.end()
    }
}

fn parse_single_record<E>(csv: &str) -> Result<StringRecord, E>
where
    E: serde::ser::Error,
{
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(false)
        .from_reader(csv.as_bytes());

    let mut record = StringRecord::new();
    match rdr.read_record(&mut record) {
        Ok(true) => Ok(()),
        Ok(false) => Err(E::custom("No data in embedded csv")),
        Err(e) => Err(E::custom(format!(
            "Failed to read record from embedded csv: {e}"
        ))),
    }?;
    match rdr.read_record(&mut StringRecord::new()) {
        Ok(false) => Ok(record),
        Ok(true) | Err(_) => Err(E::custom(format!("Multiple CSV records found (did you forget to quote a field containing a newline?) in {csv:?}"))),
    }
}

/// Helper to generate a SmartREST operation status message
pub trait OperationStatusMessage {
    fn executing() -> Message {
        Self::create_message(Self::status_executing())
    }

    fn successful(parameter: Option<&str>) -> Message {
        Self::create_message(Self::status_successful(parameter))
    }

    fn failed(failure_reason: &str) -> Message {
        Self::create_message(Self::status_failed(failure_reason))
    }

    fn create_message(payload: SmartRest) -> Message {
        let topic = C8yTopic::SmartRestResponse.to_topic().unwrap(); // never fail
        Message::new(&topic, payload)
    }

    fn status_executing() -> SmartRest;
    fn status_successful(parameter: Option<&str>) -> SmartRest;
    fn status_failed(failure_reason: &str) -> SmartRest;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_smartrest_supported_operations() {
        let smartrest = declare_supported_operations(&["c8y_SoftwareUpdate", "c8y_LogfileRequest"]);
        assert_eq!(smartrest, "114,c8y_SoftwareUpdate,c8y_LogfileRequest");
    }

    #[test]
    fn serialize_smartrest_get_pending_operations() {
        let smartrest = request_pending_operations();
        assert_eq!(smartrest, "500");
    }

    #[test]
    fn serialize_smartrest_set_operation_to_executing() {
        let smartrest = set_operation_executing(CumulocitySupportedOperations::C8ySoftwareUpdate);
        assert_eq!(smartrest, "501,c8y_SoftwareUpdate");
    }

    #[test]
    fn serialize_smartrest_set_operation_to_successful() {
        let smartrest =
            succeed_operation_no_payload(CumulocitySupportedOperations::C8ySoftwareUpdate);
        assert_eq!(smartrest, "503,c8y_SoftwareUpdate");
    }

    #[test]
    fn serialize_smartrest_set_operation_to_successful_with_payload() {
        let smartrest = succeed_static_operation(
            CumulocitySupportedOperations::C8ySoftwareUpdate,
            Some("a payload"),
        );
        assert_eq!(smartrest, "503,c8y_SoftwareUpdate,a payload");
    }

    #[test]
    fn serialize_smartrest_set_custom_operation_to_successful_with_text_payload() {
        let smartrest = succeed_operation(
            "c8y_RelayArray",
            TextOrCsv::Text("true,false,true".to_owned()),
        )
        .unwrap();
        assert_eq!(smartrest, "503,c8y_RelayArray,\"true,false,true\"");
    }

    #[test]
    fn serialize_smartrest_set_custom_operation_to_successful_with_csv_payload() {
        let smartrest = succeed_operation(
            "c8y_RelayArray",
            TextOrCsv::Csv(EmbeddedCsv("true,false,true".to_owned())),
        )
        .unwrap();
        assert_eq!(smartrest, "503,c8y_RelayArray,true,false,true");
    }

    #[test]
    fn serialize_smartrest_set_custom_operation_to_successful_with_multi_record_csv_payload() {
        succeed_operation(
            "c8y_RelayArray",
            TextOrCsv::Csv(EmbeddedCsv("true\n1,2,3".to_owned())),
        )
        .unwrap_err();
    }

    #[test]
    fn serialize_smartrest_set_custom_operation_to_successful_requotes_csv_payload() {
        let smartrest = succeed_operation(
            "c8y_RelayArray",
            TextOrCsv::Csv(EmbeddedCsv("true,random\"quote".to_owned())),
        )
        .unwrap();
        assert_eq!(smartrest, "503,c8y_RelayArray,true,\"random\"\"quote\"");
    }

    #[test]
    fn serialize_smartrest_set_operation_to_failed() {
        let smartrest = fail_operation(
            CumulocitySupportedOperations::C8ySoftwareUpdate,
            "Failed due to permission.",
        );
        assert_eq!(
            smartrest,
            "502,c8y_SoftwareUpdate,Failed due to permission."
        );
    }

    #[test]
    fn serialize_smartrest_set_custom_operation_to_failed() {
        let smartrest = fail_operation("c8y_Custom", "Something went wrong");
        assert_eq!(smartrest, "502,c8y_Custom,Something went wrong");
    }

    #[test]
    fn serialize_smartrest_set_operation_to_failed_with_quotes() {
        let smartrest = fail_operation(
            CumulocitySupportedOperations::C8ySoftwareUpdate,
            "Failed due to permi\"ssion.",
        );
        assert_eq!(
            smartrest,
            "502,c8y_SoftwareUpdate,\"Failed due to permi\"\"ssion.\""
        );
    }

    #[test]
    fn serialize_smartrest_set_operation_to_failed_with_comma_reason() {
        let smartrest = fail_operation(
            CumulocitySupportedOperations::C8ySoftwareUpdate,
            "Failed to install collectd, modbus, and golang.",
        );
        assert_eq!(
            smartrest,
            "502,c8y_SoftwareUpdate,\"Failed to install collectd, modbus, and golang.\""
        );
    }

    #[test]
    fn serialize_smartrest_set_operation_to_failed_with_empty_reason() {
        let smartrest = fail_operation(CumulocitySupportedOperations::C8ySoftwareUpdate, "");
        assert_eq!(smartrest, "502,c8y_SoftwareUpdate,");
    }
}
