use crate::error::ApiError;
use crate::messages::*;
use crate::topic::*;
use mqtt_channel::Message;
use serde::{Deserialize, Serialize};

pub type RequestId = String;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum AgentRequest {
    HealthCheck,
    SoftwareList(SoftwareListRequest),
    SoftwareUpdate(SoftwareUpdateRequest),
    DeviceRestart(RestartOperationRequest),
}

impl AgentRequest {
    pub fn request_id(&self) -> &RequestId {
        match self {
            AgentRequest::SoftwareList(r) => &r.id,
            AgentRequest::SoftwareUpdate(r) => &r.id,
            AgentRequest::DeviceRestart(r) => &r.id,
        }
    }
}

impl TryFrom<mqtt_channel::Message> for AgentRequest {
    type Error = ApiError;

    fn try_from(message: Message) -> Result<Self, Self::Error> {
        let topic = message.topic.name.as_str();
        let payload = message.payload_bytes();

        if health_check_topic_filter().accept(&message) {
            return Ok(AgentRequest::HealthCheck);
        }
        if topic == RequestTopic::SoftwareListRequest.as_str() {
            return SoftwareListRequest::from_slice(payload).map(AgentRequest::SoftwareList);
        }
        if topic == RequestTopic::SoftwareUpdateRequest.as_str() {
            return SoftwareUpdateRequest::from_slice(payload).map(AgentRequest::SoftwareUpdate);
        }
        if topic == RequestTopic::RestartRequest.as_str() {
            return RestartOperationRequest::from_slice(payload).map(AgentRequest::DeviceRestart);
        }

        Err(ApiError::UnknownTopic {
            topic: message.topic.name,
        })
    }
}
