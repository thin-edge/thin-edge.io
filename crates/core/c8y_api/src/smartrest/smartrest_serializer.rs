use crate::smartrest::csv::fields_to_csv_string;
use crate::smartrest::error::SmartRestSerializerError;
use csv::StringRecord;
use serde::ser::SerializeSeq;
use serde::Serialize;
use serde::Serializer;
use tedge_api::SoftwareListCommand;
use tedge_api::SoftwareModule;
use tracing::warn;

pub type SmartRest = String;

pub fn request_pending_operations() -> &'static str {
    "500"
}

/// Generates a SmartREST message to set the provided operation to executing
pub fn set_operation_executing_with_name(operation: impl C8yOperation) -> String {
    fields_to_csv_string(&["501", operation.name()])
}

/// Generates a SmartREST message to set the provided operation ID to executing
pub fn set_operation_executing_with_id(op_id: &str) -> String {
    fields_to_csv_string(&["504", op_id])
}

/// Generates a SmartREST message to set the provided operation to failed with the provided reason
pub fn fail_operation_with_name(operation: impl C8yOperation, reason: &str) -> String {
    fail_operation("502", operation.name(), reason)
}

/// Generates a SmartREST message to set the provided operation ID to failed with the provided reason
pub fn fail_operation_with_id(op_id: &str, reason: &str) -> String {
    fail_operation("505", op_id, reason)
}

fn fail_operation(template_id: &str, operation: &str, reason: &str) -> String {
    // If the failure reason exceeds 500 bytes, truncate it
    if reason.len() <= 500 {
        fields_to_csv_string(&[template_id, operation, reason])
    } else {
        warn!("Failure reason too long, message truncated to 500 bytes");
        fields_to_csv_string(&[template_id, operation, &reason[..500]])
    }
}

/// Generates a SmartREST message to set the provided operation to successful without a payload
pub fn succeed_operation_with_name_no_parameters(
    operation: CumulocitySupportedOperations,
) -> String {
    succeed_static_operation_with_name(operation, None::<&str>)
}

/// Generates a SmartREST message to set the provided operation to successful with an optional payload
pub fn succeed_static_operation_with_name(
    operation: CumulocitySupportedOperations,
    payload: Option<impl AsRef<str>>,
) -> String {
    succeed_static_operation("503", operation.name(), payload)
}

/// Generates a SmartREST message to set the provided operation ID to successful without a payload
pub fn succeed_operation_with_id_no_parameters(op_id: &str) -> String {
    succeed_static_operation_with_id(op_id, None::<&str>)
}

/// Generates a SmartREST message to set the provided operation ID to successful with an optional payload
pub fn succeed_static_operation_with_id(op_id: &str, payload: Option<impl AsRef<str>>) -> String {
    succeed_static_operation("506", op_id, payload)
}

fn succeed_static_operation(
    template_id: &str,
    operation: &str,
    payload: Option<impl AsRef<str>>,
) -> String {
    let mut wtr = csv::Writer::from_writer(vec![]);
    // Serialization will never fail for text
    match payload {
        Some(payload) => wtr.serialize((template_id, operation, payload.as_ref())),
        None => wtr.serialize((template_id, operation)),
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
    template_id: &str,
    operation: &str,
    reason: impl Into<TextOrCsv>,
) -> Result<String, SmartRestSerializerError> {
    let mut wtr = csv::Writer::from_writer(vec![]);
    // Serialization can fail for CSV, but not for text
    wtr.serialize((template_id, operation, reason.into()))?;
    let mut output = wtr.into_inner().unwrap();
    output.pop();
    Ok(String::from_utf8(output)?)
}

pub fn succeed_operation_with_name(
    operation: &str,
    reason: impl Into<TextOrCsv>,
) -> Result<String, SmartRestSerializerError> {
    succeed_operation("503", operation, reason)
}

pub fn succeed_operation_with_id(
    operation: &str,
    reason: impl Into<TextOrCsv>,
) -> Result<String, SmartRestSerializerError> {
    succeed_operation("506", operation, reason)
}

#[derive(Debug, Copy, Clone)]
pub enum CumulocitySupportedOperations {
    C8ySoftwareUpdate,
    C8yLogFileRequest,
    C8yRestartRequest,
    C8yUploadConfigFile,
    C8yDownloadConfigFile,
    C8yFirmware,
    C8yDeviceProfile,
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
            CumulocitySupportedOperations::C8yDeviceProfile => "c8y_DeviceProfile",
        }
    }
}

pub fn declare_supported_operations(ops: &[&str]) -> String {
    format!("114,{}", fields_to_csv_string(ops))
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
            AdvancedSoftwareList::Set(items) => Self::create_software_list("140", items),
            AdvancedSoftwareList::Append(items) => Self::create_software_list("141", items),
        };
        let list: Vec<&str> = vec.iter().map(std::ops::Deref::deref).collect();

        fields_to_csv_string(list.as_slice())
    }

    fn create_software_list(id: &str, items: Vec<SmartRestSoftwareModuleItem>) -> Vec<String> {
        if items.is_empty() {
            vec![id.into(), "".into(), "".into(), "".into(), "".into()]
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

#[cfg(test)]
mod tests {
    use super::*;
    use tedge_api::commands::SoftwareListCommandPayload;
    use tedge_api::mqtt_topics::EntityTopicId;
    use tedge_api::Jsonify;

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
        let smartrest =
            set_operation_executing_with_name(CumulocitySupportedOperations::C8ySoftwareUpdate);
        assert_eq!(smartrest, "501,c8y_SoftwareUpdate");

        let smartrest = set_operation_executing_with_id("1234");
        assert_eq!(smartrest, "504,1234");
    }

    #[test]
    fn serialize_smartrest_set_operation_to_successful() {
        let smartrest = succeed_operation_with_name_no_parameters(
            CumulocitySupportedOperations::C8ySoftwareUpdate,
        );
        assert_eq!(smartrest, "503,c8y_SoftwareUpdate");

        let smartrest = succeed_operation_with_id_no_parameters("1234");
        assert_eq!(smartrest, "506,1234");
    }

    #[test]
    fn serialize_smartrest_set_operation_to_successful_with_payload() {
        let smartrest = succeed_static_operation_with_name(
            CumulocitySupportedOperations::C8ySoftwareUpdate,
            Some("a payload"),
        );
        assert_eq!(smartrest, "503,c8y_SoftwareUpdate,a payload");

        let smartrest = succeed_static_operation_with_id("1234", Some("a payload"));
        assert_eq!(smartrest, "506,1234,a payload");
    }

    #[test]
    fn serialize_smartrest_set_custom_operation_to_successful_with_text_payload() {
        let smartrest = succeed_operation_with_name(
            "c8y_RelayArray",
            TextOrCsv::Text("true,false,true".to_owned()),
        )
        .unwrap();
        assert_eq!(smartrest, "503,c8y_RelayArray,\"true,false,true\"");
    }

    #[test]
    fn serialize_smartrest_set_custom_operation_to_successful_with_csv_payload() {
        let smartrest = succeed_operation_with_name(
            "c8y_RelayArray",
            TextOrCsv::Csv(EmbeddedCsv("true,false,true".to_owned())),
        )
        .unwrap();
        assert_eq!(smartrest, "503,c8y_RelayArray,true,false,true");
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
        assert_eq!(smartrest, "503,c8y_RelayArray,true,\"random\"\"quote\"");
    }

    #[test]
    fn serialize_smartrest_set_operation_to_failed() {
        let smartrest = fail_operation_with_name(
            CumulocitySupportedOperations::C8ySoftwareUpdate,
            "Failed due to permission.",
        );
        assert_eq!(
            smartrest,
            "502,c8y_SoftwareUpdate,Failed due to permission."
        );

        let smartrest = fail_operation_with_id("1234", "Failed due to permission.");
        assert_eq!(smartrest, "505,1234,Failed due to permission.");
    }

    #[test]
    fn serialize_smartrest_set_custom_operation_to_failed() {
        let smartrest = fail_operation_with_name("c8y_Custom", "Something went wrong");
        assert_eq!(smartrest, "502,c8y_Custom,Something went wrong");

        let smartrest = fail_operation_with_id("1234", "Something went wrong");
        assert_eq!(smartrest, "505,1234,Something went wrong");
    }

    #[test]
    fn serialize_smartrest_set_operation_to_failed_with_quotes() {
        let smartrest = fail_operation_with_name(
            CumulocitySupportedOperations::C8ySoftwareUpdate,
            "Failed due to permi\"ssion.",
        );
        assert_eq!(
            smartrest,
            "502,c8y_SoftwareUpdate,\"Failed due to permi\"\"ssion.\""
        );

        let smartrest = fail_operation_with_id("1234", "Failed due to permi\"ssion.");
        assert_eq!(smartrest, "505,1234,\"Failed due to permi\"\"ssion.\"");
    }

    #[test]
    fn serialize_smartrest_set_operation_to_failed_with_comma_reason() {
        let smartrest = fail_operation_with_name(
            CumulocitySupportedOperations::C8ySoftwareUpdate,
            "Failed to install collectd, modbus, and golang.",
        );
        assert_eq!(
            smartrest,
            "502,c8y_SoftwareUpdate,\"Failed to install collectd, modbus, and golang.\""
        );

        let smartrest =
            fail_operation_with_id("1234", "Failed to install collectd, modbus, and golang.");
        assert_eq!(
            smartrest,
            "505,1234,\"Failed to install collectd, modbus, and golang.\""
        );
    }

    #[test]
    fn serialize_smartrest_set_operation_to_failed_with_empty_reason() {
        let smartrest =
            fail_operation_with_name(CumulocitySupportedOperations::C8ySoftwareUpdate, "");
        assert_eq!(smartrest, "502,c8y_SoftwareUpdate,");

        let smartrest = fail_operation_with_id("1234", "");
        assert_eq!(smartrest, "505,1234,");
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
}
