use crate::workflow::CommandBoard;
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
            commands.insert(topic_name, (timestamp, state));
        }
        Ok(CommandBoard::new(commands))
    }
}

impl From<CommandBoard> for OnDiskCommandBoardV1 {
    fn from(board: CommandBoard) -> Self {
        let mut commands = HashMap::new();
        for (timestamp, state) in board.iter() {
            let topic_name = state.topic.name.clone();
            commands.insert(
                topic_name,
                OnDiskCommandStateV1 {
                    unix_timestamp: timestamp.unix_timestamp(),
                    status: state.status.clone(),
                    payload: state.payload.clone(),
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
