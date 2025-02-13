use super::payload::SmartrestPayload;
use super::payload::SmartrestPayloadError;
use crate::smartrest::csv::fields_to_csv_string;
use crate::smartrest::error::SmartRestSerializerError;
use csv::StringRecord;
use serde::ser::SerializeSeq;
use serde::Serialize;
use serde::Serializer;
use tedge_api::SoftwareListCommand;
use tedge_api::SoftwareModule;
use tracing::warn;

use super::message_ids::*;

pub type SmartRest = String;

pub fn request_pending_operations() -> SmartrestPayload {
    SmartrestPayload::serialize(GET_PENDING_OPERATIONS)
        .expect("shouldn't put payload over size limit")
}

/// Generates a SmartREST message to set the provided operation to executing
pub fn set_operation_executing_with_name(operation: impl C8yOperation) -> SmartrestPayload {
    SmartrestPayload::serialize((SET_OPERATION_TO_EXECUTING, operation.name()))
        .expect("operation name shouldn't put payload over size limit")
}

/// Generates a SmartREST message to set the provided operation ID to executing
pub fn set_operation_executing_with_id(op_id: &str) -> SmartrestPayload {
    SmartrestPayload::serialize((SET_OPERATION_TO_EXECUTING_ID, op_id))
        .expect("op_id shouldn't put payload over size limit")
}

/// Generates a SmartREST message to set the provided operation to failed with the provided reason
pub fn fail_operation_with_name(operation: impl C8yOperation, reason: &str) -> SmartrestPayload {
    fail_operation(SET_OPERATION_TO_FAILED, operation.name(), reason)
}

/// Generates a SmartREST message to set the provided operation ID to failed with the provided reason
pub fn fail_operation_with_id(op_id: &str, reason: &str) -> SmartrestPayload {
    fail_operation(SET_OPERATION_TO_FAILED_ID, op_id, reason)
}

fn fail_operation(template_id: usize, operation: &str, reason: &str) -> SmartrestPayload {
    // If the failure reason exceeds 500 bytes, truncate it
    if reason.len() <= 500 {
        SmartrestPayload::serialize((template_id, operation, reason))
            .expect("operation name shouldn't put payload over size limit")
    } else {
        warn!("Failure reason too long, message truncated to 500 bytes");
        SmartrestPayload::serialize((template_id, operation, &reason[..500]))
            .expect("operation name shouldn't put payload over size limit")
    }
}

/// Generates a SmartREST message to set the provided operation to successful without a payload
pub fn succeed_operation_with_name_no_parameters(
    operation: CumulocitySupportedOperations,
) -> SmartrestPayload {
    succeed_static_operation_with_name(operation, None::<&str>)
}

/// Generates a SmartREST message to set the provided operation to successful with an optional payload
pub fn succeed_static_operation_with_name(
    operation: CumulocitySupportedOperations,
    payload: Option<impl AsRef<str>>,
) -> SmartrestPayload {
    succeed_static_operation(SET_OPERATION_TO_SUCCESSFUL, operation.name(), payload)
}

/// Generates a SmartREST message to set the provided operation ID to successful without a payload
pub fn succeed_operation_with_id_no_parameters(op_id: &str) -> SmartrestPayload {
    succeed_static_operation_with_id(op_id, None::<&str>)
}

/// Generates a SmartREST message to set the provided operation ID to successful with an optional payload
pub fn succeed_static_operation_with_id(
    op_id: &str,
    payload: Option<impl AsRef<str>>,
) -> SmartrestPayload {
    succeed_static_operation(SET_OPERATION_TO_SUCCESSFUL_ID, op_id, payload)
}

fn succeed_static_operation(
    template_id: usize,
    operation: &str,
    payload: Option<impl AsRef<str>>,
) -> SmartrestPayload {
    match payload {
        Some(payload) => SmartrestPayload::serialize((template_id, operation, payload.as_ref())),
        None => SmartrestPayload::serialize((template_id, operation)),
    }
    .expect("operation name shouldn't put payload over size limit")
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
/// If the message size turns out to be bigger than maximum cumulocity message size, it will be truncated to fit in the
/// limit.
///
/// # Errors
/// This will return an error if the payload is a CSV with multiple records, or an empty CSV.
pub fn succeed_operation(
    template_id: usize,
    operation: &str,
    reason: impl Into<TextOrCsv>,
) -> Result<SmartrestPayload, SmartRestSerializerError> {
    let reason: TextOrCsv = reason.into();

    let result = SmartrestPayload::serialize((template_id, operation, &reason));

    match result {
        Ok(payload) => return Ok(payload),
        Err(SmartrestPayloadError::SerializeError(e)) => {
            return Err(SmartRestSerializerError::InvalidCsv(e))
        }
        Err(SmartrestPayloadError::TooLarge(_)) => {}
    }

    // payload too big, need to trim
    let prefix = SmartrestPayload::serialize((template_id, operation))
        .unwrap()
        .into_inner();

    let trim_indicator = "...<trimmed>";

    // 3 extra characters: 1 for comma, 2 for surrounding quotes
    let mut max_result_limit =
        super::message::MAX_PAYLOAD_LIMIT_IN_BYTES - prefix.len() - trim_indicator.len() - 3;

    // escaping can add additional characters so that the field will be too large to fit in the
    // message; if so we need to trim it after escaping, making sure we don't screw up bespoke
    // smartrest escape sequences
    let result = {
        let mut wtr = csv::WriterBuilder::new()
            .quote_style(csv::QuoteStyle::Always)
            .from_writer(vec![]);
        wtr.serialize(reason)?;
        let mut vec = wtr.into_inner().unwrap();

        // remove newline character
        vec.pop();

        // remove outer quotes, added back after trimming
        let reason = std::str::from_utf8(&vec).unwrap();
        let reason = reason.strip_prefix('"').unwrap_or(reason);
        let reason = reason.strip_suffix('"').unwrap_or(reason);

        // if we'd cut across an escaped " character, move trim point 1 char back to omit it
        if &reason[max_result_limit - 1..=max_result_limit] == r#""""# {
            max_result_limit -= 1;
        }
        let trimmed_reason = &reason[..max_result_limit];

        format!("{prefix},\"{trimmed_reason}{trim_indicator}\"")
    };

    Ok(SmartrestPayload(result))
}

pub fn succeed_operation_with_name(
    operation: &str,
    reason: impl Into<TextOrCsv>,
) -> Result<SmartrestPayload, SmartRestSerializerError> {
    succeed_operation(SET_OPERATION_TO_SUCCESSFUL, operation, reason)
}

pub fn succeed_operation_with_id(
    operation: &str,
    reason: impl Into<TextOrCsv>,
) -> Result<SmartrestPayload, SmartRestSerializerError> {
    succeed_operation(SET_OPERATION_TO_SUCCESSFUL_ID, operation, reason)
}

#[derive(Debug, Clone)]
pub enum CumulocitySupportedOperations {
    C8ySoftwareUpdate,
    C8yLogFileRequest,
    C8yRestartRequest,
    C8yUploadConfigFile,
    C8yDownloadConfigFile,
    C8yFirmware,
    C8yDeviceProfile,
    C8yCustom(String),
}

impl CumulocitySupportedOperations {
    fn as_str(&self) -> &str {
        match self {
            CumulocitySupportedOperations::C8ySoftwareUpdate => "c8y_SoftwareUpdate",
            CumulocitySupportedOperations::C8yLogFileRequest => "c8y_LogfileRequest",
            CumulocitySupportedOperations::C8yRestartRequest => "c8y_Restart",
            CumulocitySupportedOperations::C8yUploadConfigFile => "c8y_UploadConfigFile",
            CumulocitySupportedOperations::C8yDownloadConfigFile => "c8y_DownloadConfigFile",
            CumulocitySupportedOperations::C8yFirmware => "c8y_Firmware",
            CumulocitySupportedOperations::C8yDeviceProfile => "c8y_DeviceProfile",
            CumulocitySupportedOperations::C8yCustom(operation) => operation.as_str(),
        }
    }
}

pub fn declare_supported_operations(ops: &[&str]) -> SmartrestPayload {
    SmartrestPayload::serialize((SET_SUPPORTED_OPERATIONS, ops))
        .expect("TODO: ops list can increase payload over limit")
}

#[derive(Debug, Clone, PartialEq)]
pub struct SmartRestSoftwareModuleItem {
    pub name: String,
    pub version: String,
    pub software_type: String,
    pub url: String,
}

impl From<SoftwareModule> for SmartRestSoftwareModuleItem {
    fn from(module: SoftwareModule) -> Self {
        let url = match module.url {
            None => "".to_string(),
            Some(download_info) => download_info.url,
        };

        Self {
            name: module.name,
            version: module.version.unwrap_or_default(),
            software_type: module.module_type.unwrap_or(SoftwareModule::default_type()),
            url,
        }
    }
}

pub enum AdvancedSoftwareList {
    Set(Vec<SmartRestSoftwareModuleItem>),
    Append(Vec<SmartRestSoftwareModuleItem>),
}

impl AdvancedSoftwareList {
    fn smartrest_payload(self) -> String {
        let vec = match self {
            AdvancedSoftwareList::Set(items) => {
                Self::create_software_list(SET_ADVANCED_SOFTWARE_LIST, items)
            }
            AdvancedSoftwareList::Append(items) => {
                Self::create_software_list(APPEND_ADVANCED_SOFTWARE_ITEMS, items)
            }
        };
        let list: Vec<&str> = vec.iter().map(std::ops::Deref::deref).collect();

        fields_to_csv_string(list.as_slice())
    }

    fn create_software_list(id: usize, items: Vec<SmartRestSoftwareModuleItem>) -> Vec<String> {
        if items.is_empty() {
            vec![id.to_string(), "".into(), "".into(), "".into(), "".into()]
        } else {
            let mut vec = vec![id.to_string()];
            for item in items {
                vec.push(item.name);
                vec.push(item.version);
                vec.push(item.software_type);
                vec.push(item.url);
            }
            vec
        }
    }
}

pub fn get_advanced_software_list_payloads(
    software_list_cmd: &SoftwareListCommand,
    chunk_size: usize,
) -> Vec<String> {
    let mut messages: Vec<String> = Vec::new();

    if software_list_cmd.modules().is_empty() {
        messages.push(AdvancedSoftwareList::Set(vec![]).smartrest_payload());
        return messages;
    }

    let mut items: Vec<SmartRestSoftwareModuleItem> = Vec::new();
    software_list_cmd
        .modules()
        .into_iter()
        .for_each(|software_module| {
            let c8y_software_module: SmartRestSoftwareModuleItem = software_module.into();
            items.push(c8y_software_module);
        });

    let mut first = true;
    for chunk in items.chunks(chunk_size) {
        if first {
            messages.push(AdvancedSoftwareList::Set(chunk.to_vec()).smartrest_payload());
            first = false;
        } else {
            messages.push(AdvancedSoftwareList::Append(chunk.to_vec()).smartrest_payload());
        }
    }

    messages
}

/// A supported operation of the thin-edge device, used in status updates via SmartREST
///
/// This has two implementations, `&str` for custom operations, and [CumulocitySupportedOperations]
/// for statically supported operations.
pub trait C8yOperation {
    fn name(&self) -> &str;
}

impl C8yOperation for CumulocitySupportedOperations {
    fn name(&self) -> &str {
        self.as_str()
    }
}

impl C8yOperation for &str {
    fn name(&self) -> &str {
        self
    }
}

impl C8yOperation for &String {
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

#[cfg(test)]
mod tests {
    use crate::smartrest::message::MAX_PAYLOAD_LIMIT_IN_BYTES;

    use super::*;
    use tedge_api::commands::SoftwareListCommandPayload;
    use tedge_api::mqtt_topics::EntityTopicId;
    use tedge_api::Jsonify;
    use test_case::test_case;

    #[test]
    fn serialize_smartrest_supported_operations() {
        let smartrest = declare_supported_operations(&["c8y_SoftwareUpdate", "c8y_LogfileRequest"]);
        assert_eq!(
            smartrest.as_str(),
            "114,c8y_SoftwareUpdate,c8y_LogfileRequest"
        );
    }

    #[test]
    fn serialize_smartrest_get_pending_operations() {
        let smartrest = request_pending_operations();
        assert_eq!(smartrest.as_str(), "500");
    }

    #[test]
    fn serialize_smartrest_set_operation_to_executing() {
        let smartrest =
            set_operation_executing_with_name(CumulocitySupportedOperations::C8ySoftwareUpdate);
        assert_eq!(smartrest.as_str(), "501,c8y_SoftwareUpdate");

        let smartrest = set_operation_executing_with_id("1234");
        assert_eq!(smartrest.as_str(), "504,1234");
    }

    #[test]
    fn serialize_smartrest_set_operation_to_successful() {
        let smartrest = succeed_operation_with_name_no_parameters(
            CumulocitySupportedOperations::C8ySoftwareUpdate,
        );
        assert_eq!(smartrest.as_str(), "503,c8y_SoftwareUpdate");

        let smartrest = succeed_operation_with_id_no_parameters("1234");
        assert_eq!(smartrest.as_str(), "506,1234");
    }

    #[test]
    fn serialize_smartrest_set_operation_to_successful_with_payload() {
        let smartrest = succeed_static_operation_with_name(
            CumulocitySupportedOperations::C8ySoftwareUpdate,
            Some("a payload"),
        );
        assert_eq!(smartrest.as_str(), "503,c8y_SoftwareUpdate,a payload");

        let smartrest = succeed_static_operation_with_id("1234", Some("a payload"));
        assert_eq!(smartrest.as_str(), "506,1234,a payload");
    }

    #[test]
    fn serialize_smartrest_set_custom_operation_to_successful_with_text_payload() {
        let smartrest = succeed_operation_with_name(
            "c8y_RelayArray",
            TextOrCsv::Text("true,false,true".to_owned()),
        )
        .unwrap();
        assert_eq!(smartrest.as_str(), "503,c8y_RelayArray,\"true,false,true\"");
    }

    #[test]
    fn serialize_smartrest_set_custom_operation_to_successful_with_csv_payload() {
        let smartrest = succeed_operation_with_name(
            "c8y_RelayArray",
            TextOrCsv::Csv(EmbeddedCsv("true,false,true".to_owned())),
        )
        .unwrap();
        assert_eq!(smartrest.as_str(), "503,c8y_RelayArray,true,false,true");
    }

    #[test]
    fn serialize_smartrest_set_custom_operation_to_successful_with_multi_record_csv_payload() {
        succeed_operation_with_name(
            "c8y_RelayArray",
            TextOrCsv::Csv(EmbeddedCsv("true\n1,2,3".to_owned())),
        )
        .unwrap_err();
    }

    #[test]
    fn serialize_smartrest_set_custom_operation_to_successful_requotes_csv_payload() {
        let smartrest = succeed_operation_with_name(
            "c8y_RelayArray",
            TextOrCsv::Csv(EmbeddedCsv("true,random\"quote".to_owned())),
        )
        .unwrap();
        assert_eq!(
            smartrest.as_str(),
            "503,c8y_RelayArray,true,\"random\"\"quote\""
        );
    }

    #[test]
    fn serialize_smartrest_set_operation_to_failed() {
        let smartrest = fail_operation_with_name(
            CumulocitySupportedOperations::C8ySoftwareUpdate,
            "Failed due to permission.",
        );
        assert_eq!(
            smartrest.as_str(),
            "502,c8y_SoftwareUpdate,Failed due to permission."
        );

        let smartrest = fail_operation_with_id("1234", "Failed due to permission.");
        assert_eq!(smartrest.as_str(), "505,1234,Failed due to permission.");
    }

    #[test]
    fn serialize_smartrest_set_custom_operation_to_failed() {
        let smartrest = fail_operation_with_name("c8y_Custom", "Something went wrong");
        assert_eq!(smartrest.as_str(), "502,c8y_Custom,Something went wrong");

        let smartrest = fail_operation_with_id("1234", "Something went wrong");
        assert_eq!(smartrest.as_str(), "505,1234,Something went wrong");
    }

    #[test]
    fn serialize_smartrest_set_operation_to_failed_with_quotes() {
        let smartrest = fail_operation_with_name(
            CumulocitySupportedOperations::C8ySoftwareUpdate,
            "Failed due to permi\"ssion.",
        );
        assert_eq!(
            smartrest.as_str(),
            "502,c8y_SoftwareUpdate,\"Failed due to permi\"\"ssion.\""
        );

        let smartrest = fail_operation_with_id("1234", "Failed due to permi\"ssion.");
        assert_eq!(
            smartrest.as_str(),
            "505,1234,\"Failed due to permi\"\"ssion.\""
        );
    }

    #[test]
    fn serialize_smartrest_set_operation_to_failed_with_comma_reason() {
        let smartrest = fail_operation_with_name(
            CumulocitySupportedOperations::C8ySoftwareUpdate,
            "Failed to install collectd, modbus, and golang.",
        );
        assert_eq!(
            smartrest.as_str(),
            "502,c8y_SoftwareUpdate,\"Failed to install collectd, modbus, and golang.\""
        );

        let smartrest =
            fail_operation_with_id("1234", "Failed to install collectd, modbus, and golang.");
        assert_eq!(
            smartrest.as_str(),
            "505,1234,\"Failed to install collectd, modbus, and golang.\""
        );
    }

    #[test]
    fn serialize_smartrest_set_operation_to_failed_with_empty_reason() {
        let smartrest =
            fail_operation_with_name(CumulocitySupportedOperations::C8ySoftwareUpdate, "");
        assert_eq!(smartrest.as_str(), "502,c8y_SoftwareUpdate,");

        let smartrest = fail_operation_with_id("1234", "");
        assert_eq!(smartrest.as_str(), "505,1234,");
    }

    #[test]
    fn from_software_module_to_smartrest_software_module_item() {
        let software_module = SoftwareModule {
            module_type: Some("a".into()),
            name: "b".into(),
            version: Some("c".into()),
            url: Some("".into()),
            file_path: None,
        };

        let expected_c8y_item = SmartRestSoftwareModuleItem {
            name: "b".into(),
            version: "c".into(),
            software_type: "a".to_string(),
            url: "".into(),
        };

        let converted: SmartRestSoftwareModuleItem = software_module.into();
        assert_eq!(converted, expected_c8y_item);
    }

    #[test]
    fn from_thin_edge_json_to_advanced_software_list() {
        let input_json = r#"{
            "id":"1",
            "status":"successful",
            "currentSoftwareList":[ 
                {"type":"debian", "modules":[
                    {"name":"a"},
                    {"name":"b","version":"1.0"},
                    {"name":"c","url":"https://foobar.io/c.deb"},
                    {"name":"d","version":"beta","url":"https://foobar.io/d.deb"}
                ]},
                {"type":"apama","modules":[
                    {"name":"m","url":"https://foobar.io/m.epl"}
                ]}
            ]}"#;

        let command = SoftwareListCommand {
            target: EntityTopicId::default_main_device(),
            cmd_id: "1".to_string(),
            payload: SoftwareListCommandPayload::from_json(input_json).unwrap(),
        };

        let advanced_sw_list = get_advanced_software_list_payloads(&command, 2);

        assert_eq!(advanced_sw_list[0], "140,a,,debian,,b,1.0,debian,");
        assert_eq!(
            advanced_sw_list[1],
            "141,c,,debian,https://foobar.io/c.deb,d,beta,debian,https://foobar.io/d.deb"
        );
        assert_eq!(advanced_sw_list[2], "141,m,,apama,https://foobar.io/m.epl");
    }

    #[test]
    fn empty_to_advanced_list() {
        let input_json = r#"{
            "id":"1",
            "status":"successful",
            "currentSoftwareList":[]
            }"#;

        let command = &SoftwareListCommand {
            target: EntityTopicId::default_main_device(),
            cmd_id: "1".to_string(),
            payload: SoftwareListCommandPayload::from_json(input_json).unwrap(),
        };

        let advanced_sw_list = get_advanced_software_list_payloads(command, 2);
        assert_eq!(advanced_sw_list[0], "140,,,,");
    }

    /// Make sure that `reason` field is trimmed correctly, even in presence of double quote
    /// sequences.
    #[test_case(MAX_PAYLOAD_LIMIT_IN_BYTES - 1, 2; "skips_final_quote_because_wont_fit")]
    #[test_case(MAX_PAYLOAD_LIMIT_IN_BYTES - 2, 4; "preserves_final_quote_because_fits")]
    fn succeed_operation_trims_reason_field_3171(message_len: usize, expected_num_quotes: usize) {
        let prefix_len = "503,c8y_Command,".len();

        let mut reason: String = "a".repeat(message_len - prefix_len - 2);
        reason.push('"');

        let smartrest =
            succeed_operation(SET_OPERATION_TO_SUCCESSFUL, "c8y_Command", reason).unwrap();

        // assert message is under size limit and has expected structure
        assert!(
            smartrest.as_str().len() <= MAX_PAYLOAD_LIMIT_IN_BYTES,
            "bigger than message size limit: {} > {}",
            smartrest.as_str().len(),
            MAX_PAYLOAD_LIMIT_IN_BYTES
        );
        let mut fields = smartrest.as_str().split(',');
        assert_eq!(fields.next().unwrap(), "503");
        assert_eq!(fields.next().unwrap(), "c8y_Command");

        // assert trimming preserves valid double quotes
        let reason = fields.next().unwrap();

        let num_quotes = reason.chars().filter(|c| *c == '"').count();

        assert_eq!(num_quotes, expected_num_quotes);
    }
}
