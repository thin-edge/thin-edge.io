use crate::entity::EntityType;
use crate::workflow::CommandBoard;
use crate::workflow::CommandEntry;
use crate::workflow::GenericCommandState;
use mqtt_channel::Topic;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use time::OffsetDateTime;

/// Define the file format used to persist a [CommandBoard] on-disk
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "version")]
pub(crate) enum OnDiskCommandBoard {
    V1(OnDiskCommandBoardV1),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct OnDiskCommandBoardV1 {
    commands: HashMap<String, OnDiskCommandStateV1>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct OnDiskCommandStateV1 {
    unix_timestamp: i64,
    status: String,
    payload: Value,
    #[serde(default = "crate::workflow::default_entity_type")]
    entity_type: EntityType,
}

impl TryFrom<OnDiskCommandBoard> for CommandBoard {
    type Error = CommandBoardTomlError;
    fn try_from(value: OnDiskCommandBoard) -> Result<Self, Self::Error> {
        match value {
            OnDiskCommandBoard::V1(board) => board.try_into(),
        }
    }
}

impl From<CommandBoard> for OnDiskCommandBoard {
    fn from(board: CommandBoard) -> Self {
        OnDiskCommandBoard::V1(board.into())
    }
}

impl TryFrom<OnDiskCommandBoardV1> for CommandBoard {
    type Error = CommandBoardTomlError;

    fn try_from(board: OnDiskCommandBoardV1) -> Result<Self, Self::Error> {
        let mut commands = HashMap::new();
        for (topic_name, command) in board.commands {
            let topic =
                Topic::new(&topic_name).map_err(|_| CommandBoardTomlError::InvalidTopic {
                    name: topic_name.clone(),
                })?;
            let timestamp =
                OffsetDateTime::from_unix_timestamp(command.unix_timestamp).map_err(|_| {
                    CommandBoardTomlError::InvalidTimestamp {
                        value: command.unix_timestamp,
                    }
                })?;
            let state = GenericCommandState::new(topic, command.status, command.payload);
            commands.insert(
                topic_name,
                CommandEntry {
                    timestamp,
                    state,
                    entity_type: command.entity_type,
                },
            );
        }
        Ok(CommandBoard::new(commands))
    }
}

impl From<CommandBoard> for OnDiskCommandBoardV1 {
    fn from(board: CommandBoard) -> Self {
        let mut commands = HashMap::new();
        for entry in board.iter() {
            let topic_name = entry.state.topic.name.clone();
            commands.insert(
                topic_name,
                OnDiskCommandStateV1 {
                    unix_timestamp: entry.timestamp.unix_timestamp(),
                    status: entry.state.status.clone(),
                    payload: entry.state.payload.clone(),
                    entity_type: entry.entity_type,
                },
            );
        }
        OnDiskCommandBoardV1 { commands }
    }
}

/// Error reading from disk the persisted state of the commands under execution
#[derive(thiserror::Error, Debug)]
pub enum CommandBoardTomlError {
    #[error("Invalid unix timestamp: {value}")]
    InvalidTimestamp { value: i64 },

    #[error("Invalid topic name: {name}")]
    InvalidTopic { name: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_board_persisted_before_the_entity_type_holds_device_commands() {
        let file = r#"{
            "version": "V1",
            "commands": {
                "te/device/main///cmd/restart/1": {
                    "unix_timestamp": 1700000000,
                    "status": "executing",
                    "payload": { "status": "executing" }
                }
            }
        }"#;

        let board: CommandBoard = serde_json::from_str(file).unwrap();

        let entry = board.entry("te/device/main///cmd/restart/1").unwrap();
        assert_eq!(entry.entity_type, EntityType::MainDevice);
        assert_eq!(entry.state.status, "executing");
    }

    #[test]
    fn a_persisted_board_keeps_the_entity_type_of_each_command() {
        let mut board = CommandBoard::default();
        let service_cmd = GenericCommandState::new(
            Topic::new_unchecked("te/device/main/service/collectd/cmd/restart/1"),
            "executing".to_string(),
            serde_json::json!({}),
        );
        let device_cmd = GenericCommandState::new(
            Topic::new_unchecked("te/device/main///cmd/restart/2"),
            "executing".to_string(),
            serde_json::json!({}),
        );
        board.insert(EntityType::Service, service_cmd).unwrap();
        board.insert(EntityType::MainDevice, device_cmd).unwrap();

        let file = serde_json::to_string(&board).unwrap();
        let reloaded: CommandBoard = serde_json::from_str(&file).unwrap();

        assert_eq!(
            reloaded
                .entry("te/device/main/service/collectd/cmd/restart/1")
                .map(|entry| entry.entity_type),
            Some(EntityType::Service)
        );
        assert_eq!(
            reloaded
                .entry("te/device/main///cmd/restart/2")
                .map(|entry| entry.entity_type),
            Some(EntityType::MainDevice)
        );
    }
}
