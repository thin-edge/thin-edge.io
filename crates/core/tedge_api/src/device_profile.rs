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
    operation: OperationType,
    skip: bool,
    #[serde(flatten)]
    payload: OperationPayload,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OperationPayload {
    #[serde(rename = "payload")]
    Firmware(FirmwarePayload),
    #[serde(rename = "payload")]
    Software(SoftwarePayload),
    #[serde(rename = "payload")]
    Config(ConfigPayload),
}

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FirmwarePayload {
    pub name: Option<String>,
    pub version: Option<String>,
    pub remote_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SoftwarePayload {
    pub update_list: Vec<SoftwareRequestResponseSoftwareList>,
}

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConfigPayload {
    #[serde(rename = "type")]
    pub config_type: String,
    pub remote_url: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DeviceProfileInfo {
    pub name: Option<String>,
    pub version: Option<String>,
}

impl Jsonify for DeviceProfileInfo {}

impl DeviceProfileCmdPayload {
    pub fn add_firmware(&mut self, firmware: FirmwarePayload) {
        let firmware_operation = DeviceProfileOperation {
            operation: OperationType::FirmwareUpdate,
            skip: false,
            payload: OperationPayload::Firmware(firmware),
        };

        self.operations.push(firmware_operation);
    }

    pub fn add_software(&mut self, software: SoftwarePayload) {
        let software_operation = DeviceProfileOperation {
            operation: OperationType::SoftwareUpdate,
            skip: false,
            payload: OperationPayload::Software(software),
        };

        self.operations.push(software_operation);
    }

    pub fn add_config(&mut self, config: ConfigPayload) {
        let config_operation = DeviceProfileOperation {
            operation: OperationType::ConfigUpdate,
            skip: false,
            payload: OperationPayload::Config(config),
        };

        self.operations.push(config_operation);
    }
}
