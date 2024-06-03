use crate::error::SoftwareError;
use crate::mqtt_topics::Channel;
use crate::mqtt_topics::EntityTopicError;
use crate::mqtt_topics::EntityTopicId;
use crate::mqtt_topics::MqttSchema;
use crate::mqtt_topics::OperationType;
use crate::software::*;
use crate::workflow::GenericCommandData;
use crate::workflow::GenericCommandState;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use download::DownloadInfo;
use log::error;
use mqtt_channel::MqttError;
use mqtt_channel::MqttMessage;
use mqtt_channel::QoS;
use mqtt_channel::Topic;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use std::fmt;
use time::OffsetDateTime;

/// A command instance with its target and its current state of execution
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct Command<Payload> {
    pub target: EntityTopicId,
    pub cmd_id: String,
    pub payload: Payload,
}

impl<Payload> Command<Payload>
where
    Payload: Default,
{
    /// Build a new command with a given id
    pub fn new(target: &EntityTopicId, cmd_id: String) -> Self {
        Command {
            target: target.clone(),
            cmd_id,
            payload: Default::default(),
        }
    }
}

impl<Payload> Command<Payload>
where
    Payload: CommandPayload,
{
    /// Return the MQTT topic identifier of the target
    fn topic_id(&self) -> &EntityTopicId {
        &self.target
    }

    /// Return the MQTT channel for this command
    fn channel(&self) -> Channel {
        Channel::Command {
            operation: Payload::operation_type(),
            cmd_id: self.cmd_id.clone(),
        }
    }

    /// Return the MQTT topic for this command
    pub fn topic(&self, schema: &MqttSchema) -> Topic {
        schema.topic_for(self.topic_id(), &self.channel())
    }

    /// Return the current status of the command
    pub fn status(&self) -> CommandStatus {
        self.payload.status()
    }

    /// Set the status of the command
    pub fn with_status(mut self, status: CommandStatus) -> Self {
        self.payload.set_status(status);
        self
    }

    /// Set the failure reason of the command
    pub fn with_error(mut self, reason: String) -> Self {
        self.payload.set_error(reason);
        self
    }

    /// Return the MQTT message to register support for this types of command
    pub fn capability_message(schema: &MqttSchema, target: &EntityTopicId) -> MqttMessage {
        let meta_topic = schema.capability_topic_for(target, Payload::operation_type());
        let payload = "{}";
        MqttMessage::new(&meta_topic, payload)
            .with_retain()
            .with_qos(QoS::AtLeastOnce)
    }

    /// Mark the command as executing
    pub fn executing(&mut self) {
        self.payload.executing();
    }

    /// Mark the command as successful
    pub fn successful(&mut self) {
        self.payload.successful();
    }

    /// Mark the command as failed
    pub fn failed(&mut self, reason: impl Into<String>) {
        self.payload.failed(reason);
    }
}

impl<Payload> Command<Payload>
where
    Payload: DeserializeOwned,
{
    /// Return the Command from a JSON payload
    pub fn try_from_json(
        target: EntityTopicId,
        cmd_id: String,
        json: serde_json::Value,
    ) -> Result<Self, serde_json::Error> {
        let payload = serde_json::from_value(json)?;
        Ok(Command {
            target,
            cmd_id,
            payload,
        })
    }
}

impl<Payload> Command<Payload>
where
    Payload: Jsonify + DeserializeOwned + Serialize + CommandPayload,
{
    /// Parse a command received from MQTT
    pub fn parse(
        schema: &MqttSchema,
        message: MqttMessage,
    ) -> Result<Option<Self>, CommandParsingError> {
        let (target, channel) = schema.entity_channel_of(message.topic.as_ref())?;
        let cmd_id = match channel {
            Channel::Command { operation, cmd_id } if operation == Payload::operation_type() => {
                cmd_id
            }
            Channel::Command { operation, .. } => {
                return Err(CommandParsingError::InvalidCommandType {
                    actual: operation.to_string(),
                    expected: Payload::operation_type().to_string(),
                })
            }
            _ => {
                return Err(CommandParsingError::InvalidCommandTopic {
                    topic: message.topic.name.clone(),
                })
            }
        };

        Ok(Self::try_from_bytes(target, cmd_id, message.payload())?)
    }

    /// Return the Command received on a topic
    pub fn try_from_bytes(
        target: EntityTopicId,
        cmd_id: String,
        bytes: &[u8],
    ) -> Result<Option<Self>, serde_json::Error> {
        if bytes.is_empty() {
            Ok(None)
        } else {
            let payload = Payload::from_slice(bytes)?;
            Ok(Some(Command {
                target,
                cmd_id,
                payload,
            }))
        }
    }

    /// Return the generic command representation for this command
    pub fn into_generic_command(self, schema: &MqttSchema) -> GenericCommandState {
        let topic = self.topic(schema);
        let status = self.status().to_string();
        let payload = self.payload.to_value();
        GenericCommandState::new(topic, status, payload)
    }

    /// Return the MQTT message for this command
    pub fn command_message(&self, schema: &MqttSchema) -> MqttMessage {
        let topic = self.topic(schema);
        let payload = self.payload.to_bytes();
        MqttMessage::new(&topic, payload)
            .with_qos(QoS::AtLeastOnce)
            .with_retain()
    }

    /// Return the MQTT message to clear this command
    pub fn clearing_message(&self, schema: &MqttSchema) -> MqttMessage {
        let topic = self.topic(schema);
        MqttMessage::new(&topic, vec![])
            .with_qos(QoS::AtLeastOnce)
            .with_retain()
    }
}

impl<Payload> From<Command<Payload>> for GenericCommandState
where
    Payload: Jsonify + DeserializeOwned + Serialize + CommandPayload,
{
    fn from(value: Command<Payload>) -> Self {
        // FIXME A Command payload should know its root topic
        let schema = MqttSchema::default();
        value.into_generic_command(&schema)
    }
}

impl<Payload> From<Command<Payload>> for GenericCommandData
where
    Payload: Jsonify + DeserializeOwned + Serialize + CommandPayload,
{
    fn from(value: Command<Payload>) -> Self {
        GenericCommandData::State(value.into())
    }
}

impl<Payload> TryFrom<GenericCommandState> for Command<Payload>
where
    Payload: DeserializeOwned + CommandPayload,
{
    type Error = String;

    fn try_from(value: GenericCommandState) -> Result<Self, Self::Error> {
        let Some(target) = value.target().and_then(|t| t.parse().ok()) else {
            return Err(format!("Not an operation topic: {}", value.topic.as_ref()));
        };
        let Some(cmd_id) = value.cmd_id() else {
            return Err(format!("Not an operation topic: {}", value.topic.as_ref()));
        };

        Command::<Payload>::try_from_json(target, cmd_id, value.payload).map_err(|err| {
            format!(
                "Incorrect {operation} request payload: {err}",
                operation = Payload::operation_type()
            )
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CommandParsingError {
    #[error(transparent)]
    InvalidTopic(#[from] EntityTopicError),

    #[error("Not a command topic: {topic}")]
    InvalidCommandTopic { topic: String },

    #[error("Not the expected command type: {actual} instead of {expected}")]
    InvalidCommandType { actual: String, expected: String },

    #[error(transparent)]
    InvalidPayload(#[from] MqttError),

    #[error(transparent)]
    InvalidCommandPayload(#[from] serde_json::Error),
}

/// A command payload describing the current state of a command
pub trait CommandPayload {
    /// Return the operation type shared by all these commands
    fn operation_type() -> OperationType;

    /// Return the current status of the command
    fn status(&self) -> CommandStatus;

    /// Set the status of the command
    fn set_status(&mut self, status: CommandStatus);

    /// Set the failure reason of the command
    fn set_error(&mut self, reason: impl Into<String>) {
        self.set_status(CommandStatus::Failed {
            reason: reason.into(),
        });
    }

    /// Mark the command as executing
    fn executing(&mut self) {
        self.set_status(CommandStatus::Executing);
    }

    /// Mark the command as successful
    fn successful(&mut self) {
        self.set_status(CommandStatus::Successful);
    }

    /// Mark the command as failed
    fn failed(&mut self, reason: impl Into<String>) {
        self.set_status(CommandStatus::Failed {
            reason: reason.into(),
        });
    }
}

/// All the messages are serialized using json.
pub trait Jsonify {
    fn from_json(json_str: &str) -> Result<Self, serde_json::Error>
    where
        Self: DeserializeOwned,
    {
        serde_json::from_str(json_str)
    }

    fn from_slice(bytes: &[u8]) -> Result<Self, serde_json::Error>
    where
        Self: DeserializeOwned,
    {
        serde_json::from_slice(bytes)
    }

    fn to_value(&self) -> Value
    where
        Self: Serialize,
    {
        serde_json::to_value(self).unwrap() // all thin-edge data can be serialized to json
    }

    fn to_json(&self) -> String
    where
        Self: Serialize,
    {
        serde_json::to_string(self).unwrap() // all thin-edge data can be serialized to json
    }

    fn to_bytes(&self) -> Vec<u8>
    where
        Self: Serialize,
    {
        serde_json::to_vec(self).unwrap() // all thin-edge data can be serialized to json
    }
}

/// Command to request the list of software packages that are installed on a device
pub type SoftwareListCommand = Command<SoftwareListCommandPayload>;

/// Payload of a [SoftwareListCommand]
#[derive(Debug, Clone, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SoftwareListCommandPayload {
    #[serde(flatten)]
    pub status: CommandStatus,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub current_software_list: Vec<SoftwareList>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_path: Option<Utf8PathBuf>,
}

impl Jsonify for SoftwareListCommandPayload {}

impl CommandPayload for SoftwareListCommandPayload {
    fn operation_type() -> OperationType {
        OperationType::SoftwareList
    }

    fn status(&self) -> CommandStatus {
        self.status.clone()
    }

    fn set_status(&mut self, status: CommandStatus) {
        self.status = status
    }
}

/// Sub list of modules grouped by plugin type.
#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct SoftwareList {
    #[serde(rename = "type")]
    pub plugin_type: SoftwareType,
    pub modules: Vec<SoftwareModuleItem>,
}

impl SoftwareListCommand {
    /// Add a list of packages all of the same type
    pub fn add_modules(&mut self, plugin_type: SoftwareType, modules: Vec<SoftwareModule>) {
        let modules = modules.into_iter().map(|module| module.into()).collect();
        self.payload.current_software_list.push(SoftwareList {
            plugin_type,
            modules,
        });
    }

    /// List all the packages
    pub fn modules(&self) -> Vec<SoftwareModule> {
        self.payload
            .current_software_list
            .iter()
            .flat_map(|list| {
                let plugin_type = &list.plugin_type;
                list.modules
                    .clone()
                    .into_iter()
                    .map(|module| SoftwareModule {
                        module_type: Some(plugin_type.clone()),
                        name: module.name,
                        version: module.version,
                        url: module.url,
                        file_path: None,
                    })
            })
            .collect()
    }
}

/// Command to install/remove software packages on a device
pub type SoftwareUpdateCommand = Command<SoftwareUpdateCommandPayload>;

/// Payload of a [SoftwareUpdateCommand]
#[derive(Debug, Clone, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SoftwareUpdateCommandPayload {
    #[serde(flatten)]
    pub status: CommandStatus,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub update_list: Vec<SoftwareRequestResponseSoftwareList>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub failures: Vec<SoftwareRequestResponseSoftwareList>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_path: Option<Utf8PathBuf>,
}

impl Jsonify for SoftwareUpdateCommandPayload {}

impl CommandPayload for SoftwareUpdateCommandPayload {
    fn operation_type() -> OperationType {
        OperationType::SoftwareUpdate
    }

    fn status(&self) -> CommandStatus {
        self.status.clone()
    }

    fn set_status(&mut self, status: CommandStatus) {
        self.status = status
    }
}

impl SoftwareUpdateCommand {
    pub fn add_update(&mut self, mut update: SoftwareModuleUpdate) {
        update.normalize();
        let plugin_type = update
            .module()
            .module_type
            .clone()
            .unwrap_or_else(SoftwareModule::default_type);

        if let Some(list) = self
            .payload
            .update_list
            .iter_mut()
            .find(|list| list.plugin_type == plugin_type)
        {
            list.modules.push(update.into());
        } else {
            self.payload
                .update_list
                .push(SoftwareRequestResponseSoftwareList {
                    plugin_type,
                    modules: vec![update.into()],
                });
        }
    }

    pub fn add_updates(&mut self, plugin_type: &str, updates: Vec<SoftwareModuleUpdate>) {
        self.payload
            .update_list
            .push(SoftwareRequestResponseSoftwareList {
                plugin_type: plugin_type.to_string(),
                modules: updates
                    .into_iter()
                    .map(|update| update.into())
                    .collect::<Vec<SoftwareModuleItem>>(),
            })
    }

    pub fn modules_types(&self) -> Vec<SoftwareType> {
        let mut modules_types = vec![];

        for updates_per_type in self.payload.update_list.iter() {
            modules_types.push(updates_per_type.plugin_type.clone())
        }

        modules_types
    }

    pub fn updates_for(&self, module_type: &str) -> Vec<SoftwareModuleUpdate> {
        let mut updates = vec![];

        if let Some(items) = self
            .payload
            .update_list
            .iter()
            .find(|&items| items.plugin_type == module_type)
        {
            for item in items.modules.iter() {
                let module = SoftwareModule::new(
                    Some(module_type.to_string()),
                    item.name.clone(),
                    item.version.clone(),
                    item.url.clone(),
                    None,
                );

                match item.action {
                    None => {}
                    Some(SoftwareModuleAction::Install) => {
                        updates.push(SoftwareModuleUpdate::install(module));
                    }
                    Some(SoftwareModuleAction::Remove) => {
                        updates.push(SoftwareModuleUpdate::remove(module));
                    }
                }
            }
        }

        updates
    }

    pub fn add_errors(&mut self, plugin_type: &str, errors: Vec<SoftwareError>) {
        self.payload
            .failures
            .push(SoftwareRequestResponseSoftwareList {
                plugin_type: plugin_type.to_string(),
                modules: errors
                    .into_iter()
                    .filter_map(|update| update.into())
                    .collect::<Vec<SoftwareModuleItem>>(),
            })
    }

    pub fn set_log_path(&mut self, path: impl AsRef<Utf8Path>) {
        self.payload.log_path = Some(path.as_ref().into())
    }
}

/// Sub list of modules grouped by plugin type.
#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct SoftwareRequestResponseSoftwareList {
    #[serde(rename = "type")]
    pub plugin_type: SoftwareType,
    pub modules: Vec<SoftwareModuleItem>,
}

/// Variants represent Software Operations Supported actions.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SoftwareModuleAction {
    Install,
    Remove,
}

/// Software module payload definition.
#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct SoftwareModuleItem {
    pub name: SoftwareName,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<SoftwareVersion>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(flatten)]
    pub url: Option<DownloadInfo>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<SoftwareModuleAction>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl From<SoftwareModule> for SoftwareModuleItem {
    fn from(module: SoftwareModule) -> Self {
        SoftwareModuleItem {
            name: module.name,
            version: module.version,
            url: module.url,
            action: None,
            reason: None,
        }
    }
}

impl From<SoftwareModuleUpdate> for SoftwareModuleItem {
    fn from(update: SoftwareModuleUpdate) -> Self {
        match update {
            SoftwareModuleUpdate::Install { module } => SoftwareModuleItem {
                name: module.name,
                version: module.version,
                url: module.url,
                action: Some(SoftwareModuleAction::Install),
                reason: None,
            },
            SoftwareModuleUpdate::Remove { module } => SoftwareModuleItem {
                name: module.name,
                version: module.version,
                url: module.url,
                action: Some(SoftwareModuleAction::Remove),
                reason: None,
            },
        }
    }
}

impl From<SoftwareError> for Option<SoftwareModuleItem> {
    fn from(error: SoftwareError) -> Self {
        match error {
            SoftwareError::Install { module, reason } => Some(SoftwareModuleItem {
                name: module.name,
                version: module.version,
                url: module.url,
                action: Some(SoftwareModuleAction::Install),
                reason: Some(reason),
            }),
            SoftwareError::Remove { module, reason } => Some(SoftwareModuleItem {
                name: module.name,
                version: module.version,
                url: module.url,
                action: Some(SoftwareModuleAction::Remove),
                reason: Some(reason),
            }),
            _ => None,
        }
    }
}

/// Payload of SoftwareList and SoftwareUpdate commands metadata
#[derive(Debug, Clone, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SoftwareCommandMetadata {
    #[serde(default)]
    pub types: Vec<SoftwareType>,
}

impl Jsonify for SoftwareCommandMetadata {}

/// Command to restart a device
pub type RestartCommand = Command<RestartCommandPayload>;

/// Command to restart a device
#[derive(Debug, Clone, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RestartCommandPayload {
    #[serde(flatten)]
    pub status: CommandStatus,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_path: Option<Utf8PathBuf>,
}

impl RestartCommandPayload {
    pub fn new(status: CommandStatus) -> Self {
        RestartCommandPayload {
            status,
            log_path: None,
        }
    }
}

impl Jsonify for RestartCommandPayload {}

impl CommandPayload for RestartCommandPayload {
    fn operation_type() -> OperationType {
        OperationType::Restart
    }

    fn status(&self) -> CommandStatus {
        self.status.clone()
    }

    fn set_status(&mut self, status: CommandStatus) {
        self.status = status
    }
}

#[derive(Debug, Default, Deserialize, Serialize, PartialEq, Eq, Clone)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum CommandStatus {
    #[default]
    Init,
    Scheduled,
    Executing,
    Successful,
    Failed {
        #[serde(default = "default_failure_reason")]
        reason: String,
    },

    /// Unknown status used by a custom workflow
    #[serde(other)]
    Unknown,
}

impl CommandStatus {
    pub fn is_terminal_status(&self) -> bool {
        matches!(
            self,
            CommandStatus::Successful | CommandStatus::Failed { reason: _ }
        )
    }

    pub fn is_successful(&self) -> bool {
        *self == CommandStatus::Successful
    }

    pub fn is_failed(&self) -> bool {
        matches!(self, CommandStatus::Failed { reason: _ })
    }
}

fn default_failure_reason() -> String {
    "Unknown reason".to_string()
}

impl fmt::Display for CommandStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let str = match self {
            CommandStatus::Init => "init",
            CommandStatus::Scheduled => "scheduled",
            CommandStatus::Executing => "executing",
            CommandStatus::Successful => "successful",
            CommandStatus::Failed { .. } => "failed",
            CommandStatus::Unknown => "unknown",
        };
        str.fmt(f)
    }
}

/// TODO: Deprecate OperationStatus
#[derive(Debug, Deserialize, Serialize, PartialEq, Copy, Eq, Clone)]
#[serde(rename_all = "camelCase")]
pub enum OperationStatus {
    Successful,
    Failed,
    Executing,
}

/// Command to request a log file to be uploaded
pub type LogUploadCmd = Command<LogUploadCmdPayload>;

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LogMetadata {
    pub types: Vec<String>,
}

impl Jsonify for LogMetadata {}

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LogUploadCmdPayload {
    #[serde(flatten)]
    pub status: CommandStatus,
    pub tedge_url: String,
    #[serde(rename = "type")]
    pub log_type: String,
    #[serde(with = "time::serde::rfc3339")]
    pub date_from: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub date_to: OffsetDateTime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_text: Option<String>,
    pub lines: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_path: Option<Utf8PathBuf>,
}

impl Jsonify for LogUploadCmdPayload {}

impl CommandPayload for LogUploadCmdPayload {
    fn operation_type() -> OperationType {
        OperationType::LogUpload
    }

    fn status(&self) -> CommandStatus {
        self.status.clone()
    }

    fn set_status(&mut self, status: CommandStatus) {
        self.status = status
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LogUploadCmdMetadata {
    #[serde(default)]
    pub types: Vec<String>,
}

impl Jsonify for LogUploadCmdMetadata {}

/// Command to request a configuration snapshot to be uploaded
pub type ConfigSnapshotCmd = Command<ConfigSnapshotCmdPayload>;

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConfigMetadata {
    pub types: Vec<String>,
}

impl Jsonify for ConfigMetadata {}

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConfigSnapshotCmdPayload {
    #[serde(flatten)]
    pub status: CommandStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tedge_url: Option<String>,
    #[serde(rename = "type")]
    pub config_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_path: Option<Utf8PathBuf>,
}

impl Jsonify for ConfigSnapshotCmdPayload {}

impl CommandPayload for ConfigSnapshotCmdPayload {
    fn operation_type() -> OperationType {
        OperationType::ConfigSnapshot
    }

    fn status(&self) -> CommandStatus {
        self.status.clone()
    }

    fn set_status(&mut self, status: CommandStatus) {
        self.status = status
    }
}

impl ConfigSnapshotCmdPayload {
    pub fn executing(&mut self, tedge_url: Option<String>) {
        self.status = CommandStatus::Executing;
        if tedge_url.is_some() {
            self.tedge_url = tedge_url;
        }
    }

    pub fn successful(&mut self, path: impl Into<String>) {
        self.status = CommandStatus::Successful;
        self.path = Some(path.into())
    }
}

/// Command to request a configuration to be updated
pub type ConfigUpdateCmd = Command<ConfigUpdateCmdPayload>;

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConfigUpdateCmdPayload {
    #[serde(flatten)]
    pub status: CommandStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tedge_url: Option<String>,
    pub remote_url: String,
    #[serde(rename = "type")]
    pub config_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_path: Option<Utf8PathBuf>,
}

impl Jsonify for ConfigUpdateCmdPayload {}

impl CommandPayload for ConfigUpdateCmdPayload {
    fn operation_type() -> OperationType {
        OperationType::ConfigUpdate
    }

    fn status(&self) -> CommandStatus {
        self.status.clone()
    }

    fn set_status(&mut self, status: CommandStatus) {
        self.status = status
    }
}

impl ConfigUpdateCmdPayload {
    pub fn successful(&mut self, path: impl Into<String>) {
        self.status = CommandStatus::Successful;
        self.path = Some(path.into())
    }
}

/// Command to update the device firmware
pub type FirmwareUpdateCmd = Command<FirmwareUpdateCmdPayload>;

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FirmwareInfo {
    pub name: Option<String>,
    pub version: Option<String>,
    #[serde(rename = "url")]
    pub remote_url: Option<String>,
}

impl Jsonify for FirmwareInfo {}

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FirmwareUpdateCmdPayload {
    #[serde(flatten)]
    pub status: CommandStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tedge_url: Option<String>,
    pub remote_url: String,
    pub name: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_path: Option<Utf8PathBuf>,
}

impl Jsonify for FirmwareUpdateCmdPayload {}

impl CommandPayload for FirmwareUpdateCmdPayload {
    fn operation_type() -> OperationType {
        OperationType::FirmwareUpdate
    }

    fn status(&self) -> CommandStatus {
        self.status.clone()
    }

    fn set_status(&mut self, status: CommandStatus) {
        self.status = status
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_software_request_list() {
        let request = SoftwareListCommandPayload {
            status: CommandStatus::Init,
            current_software_list: vec![],
            log_path: None,
        };
        let expected_json = r#"{"status":"init"}"#;

        let actual_json = request.to_json();

        assert_eq!(actual_json, expected_json);

        let de_request = SoftwareListCommandPayload::from_json(actual_json.as_str())
            .expect("failed to deserialize");
        assert_eq!(request, de_request);
    }

    #[test]
    fn serde_software_request_update() {
        let debian_module1 = SoftwareModuleItem {
            name: "debian1".into(),
            version: Some("0.0.1".into()),
            action: Some(SoftwareModuleAction::Install),
            url: None,
            reason: None,
        };

        let debian_module2 = SoftwareModuleItem {
            name: "debian2".into(),
            version: Some("0.0.2".into()),
            action: Some(SoftwareModuleAction::Install),
            url: None,
            reason: None,
        };

        let debian_list = SoftwareRequestResponseSoftwareList {
            plugin_type: "debian".into(),
            modules: vec![debian_module1, debian_module2],
        };

        let docker_module1 = SoftwareModuleItem {
            name: "docker1".into(),
            version: Some("0.0.1".into()),
            action: Some(SoftwareModuleAction::Remove),
            url: Some("test.com".into()),
            reason: None,
        };

        let docker_list = SoftwareRequestResponseSoftwareList {
            plugin_type: "docker".into(),
            modules: vec![docker_module1],
        };

        let request = SoftwareUpdateCommandPayload {
            status: CommandStatus::Init,
            update_list: vec![debian_list, docker_list],
            failures: vec![],
            log_path: None,
        };

        let expected_json = r#"{"status":"init","updateList":[{"type":"debian","modules":[{"name":"debian1","version":"0.0.1","action":"install"},{"name":"debian2","version":"0.0.2","action":"install"}]},{"type":"docker","modules":[{"name":"docker1","version":"0.0.1","url":"test.com","action":"remove"}]}]}"#;

        let actual_json = request.to_json();
        assert_eq!(actual_json, expected_json);

        let parsed_request = SoftwareUpdateCommandPayload::from_json(&actual_json)
            .expect("Fail to parse the json request");
        assert_eq!(parsed_request, request);
    }

    #[test]
    fn serde_custom_command_status() {
        let request = SoftwareListCommandPayload {
            status: CommandStatus::Unknown,
            current_software_list: vec![],
            log_path: None,
        };

        // The `CommandStatus::Unknown` variant is used when the status is unknown.
        // This is notably the case when the status is produced by a custom operation workflow.
        assert_eq!(
            request,
            SoftwareListCommandPayload::from_json(r#"{"status":"some-custom-status"}"#).unwrap()
        );

        // However, if serialized again the custom status is lost
        assert_eq!(request.to_json(), r#"{"status":"unknown"}"#);
    }
}
