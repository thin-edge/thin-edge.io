use crate::commands::Command;
use crate::commands::CommandPayload;
use crate::commands::SoftwareRequestResponseSoftwareList;
use crate::mqtt_topics::OperationType;
use crate::CommandStatus;
use crate::Jsonify;

use serde::Deserialize;
use serde::Serialize;

/// Command for device profile
pub type DeviceProfileCmd = Command<DeviceProfileCmdPayload>;

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DeviceProfileCmdPayload {
    #[serde(flatten)]
    pub status: CommandStatus,
    pub name: String,
    pub operations: Vec<DeviceProfileOperation>,
}

impl Jsonify for DeviceProfileCmdPayload {}

impl CommandPayload for DeviceProfileCmdPayload {
    fn operation_type() -> OperationType {
        OperationType::DeviceProfile
    }

    fn status(&self) -> CommandStatus {
        self.status.clone()
    }

    fn set_status(&mut self, status: CommandStatus) {
        self.status = status
    }
}

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DeviceProfileOperation {
    #[serde(flatten)]
    pub operation: OperationPayload,
    pub skip: bool,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "operation", content = "payload")]
pub enum OperationPayload {
    #[serde(rename = "firmware_update")]
    Firmware(FirmwarePayload),
    #[serde(rename = "software_update")]
    Software(SoftwarePayload),
    #[serde(rename = "config_update")]
    Config(ConfigPayload),
}

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FirmwarePayload {
    pub name: Option<String>,
    pub version: Option<String>,
    pub remote_url: Option<String>,
}

impl Jsonify for FirmwarePayload {}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SoftwarePayload {
    pub update_list: Vec<SoftwareRequestResponseSoftwareList>,
}

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConfigPayload {
    pub name: String,
    #[serde(rename = "type")]
    pub config_type: String,
    pub remote_url: Option<String>,
    pub server_url: Option<String>,
}

impl DeviceProfileCmdPayload {
    pub fn add_firmware(&mut self, firmware: FirmwarePayload) {
        let firmware_operation = DeviceProfileOperation {
            operation: OperationPayload::Firmware(firmware),
            skip: false,
        };

        self.operations.push(firmware_operation);
    }

    pub fn add_software(&mut self, software: SoftwarePayload) {
        let software_operation = DeviceProfileOperation {
            operation: OperationPayload::Software(software),
            skip: false,
        };

        self.operations.push(software_operation);
    }

    pub fn add_config(&mut self, config: ConfigPayload) {
        let config_operation = DeviceProfileOperation {
            operation: OperationPayload::Config(config),
            skip: false,
        };

        self.operations.push(config_operation);
    }
}
