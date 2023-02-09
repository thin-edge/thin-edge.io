use crate::download::DownloadFirmwareStatusMessage;
use c8y_api::smartrest::smartrest_serializer::TryIntoOperationStatusMessage;
use c8y_api::smartrest::topic::C8yTopic;
use mqtt_channel::Message;
use mqtt_channel::SinkExt;
use mqtt_channel::Topic;
use mqtt_channel::UnboundedSender;
use serde::Deserialize;

#[derive(Debug, Eq, PartialEq, Default, Clone, Deserialize, Hash)]
#[serde(deny_unknown_fields)]
pub struct FirmwareEntry {
    pub name: String,
    pub version: String,
    pub url: String,
    pub sha256: String,
}

impl FirmwareEntry {
    pub fn new(name: &str, version: &str, url: &str, sha256: &str) -> Self {
        Self {
            name: name.to_string(),
            version: version.to_string(),
            url: url.to_string(),
            sha256: sha256.to_string(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum ActiveOperationState {
    Pending,
    Executing,
}

pub async fn mark_pending_firmware_operation_failed(
    mut mqtt_publisher: UnboundedSender<Message>,
    child_id: impl ToString,
    op_state: ActiveOperationState,
    failure_reason: impl ToString,
) -> Result<(), anyhow::Error> {
    let c8y_child_topic =
        Topic::new_unchecked(&C8yTopic::ChildSmartRestResponse(child_id.to_string()).to_string());

    let executing_msg = Message::new(
        &c8y_child_topic,
        DownloadFirmwareStatusMessage::status_executing()?,
    );
    let failed_msg = Message::new(
        &c8y_child_topic,
        DownloadFirmwareStatusMessage::status_failed(failure_reason.to_string())?,
    );

    if op_state == ActiveOperationState::Pending {
        mqtt_publisher.send(executing_msg).await?;
    }

    mqtt_publisher.send(failed_msg).await?;

    Ok(())
}
