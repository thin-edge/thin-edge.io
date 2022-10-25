use mqtt_channel::Message;

use crate::{
    child_device::get_operation_name_from_child_topic, error::ChildDeviceConfigManagementError,
};

pub enum ConfigOperation {
    Snapshot,
    Update,
}

impl TryFrom<&Message> for ConfigOperation {
    type Error = ChildDeviceConfigManagementError;

    fn try_from(message: &Message) -> Result<Self, Self::Error> {
        let operation_name = get_operation_name_from_child_topic(&message.topic.name)?;

        if operation_name == "config_snapshot" {
            Ok(Self::Snapshot)
        } else if operation_name == "config_update" {
            Ok(Self::Update)
        } else {
            Err(
                ChildDeviceConfigManagementError::InvalidTopicFromChildOperation {
                    topic: message.topic.name.clone(),
                },
            )
        }
    }
}
