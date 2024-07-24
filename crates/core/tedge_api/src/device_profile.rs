use crate::commands::Command;
use crate::commands::CommandPayload;
use crate::commands::ConfigInfo;
use crate::commands::FirmwareInfo;
use crate::commands::SoftwareInfo;
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
    Firmware(FirmwareInfo),
    #[serde(rename = "payload")]
    Software(SoftwareInfo),
    #[serde(rename = "payload")]
    Config(ConfigInfo),
}
