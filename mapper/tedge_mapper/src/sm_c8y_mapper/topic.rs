use crate::sm_c8y_mapper::error::MapperTopicError;
use mqtt_client::{MqttClientError, Topic};
use std::convert::{TryFrom, TryInto};

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum IncomingTopic {
    SoftwareListResponse,
    SoftwareUpdateResponse,
    SmartRestRequest,
}

impl IncomingTopic {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::SoftwareListResponse => r#"tedge/commands/res/software/list"#,
            Self::SoftwareUpdateResponse => r#"tedge/commands/res/software/update"#,
            Self::SmartRestRequest => r#"c8y/s/ds"#,
        }
    }
}

impl TryFrom<String> for IncomingTopic {
    type Error = MapperTopicError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            r#"tedge/commands/res/software/list"# => Ok(IncomingTopic::SoftwareListResponse),
            r#"tedge/commands/res/software/update"# => Ok(IncomingTopic::SoftwareUpdateResponse),
            r#"c8y/s/ds"# => Ok(IncomingTopic::SmartRestRequest),
            err => Err(MapperTopicError::UnknownTopic {
                topic: err.to_string(),
            }),
        }
    }
}

impl TryFrom<&str> for IncomingTopic {
    type Error = MapperTopicError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::try_from(value.to_string())
    }
}

impl TryFrom<Topic> for IncomingTopic {
    type Error = MapperTopicError;

    fn try_from(value: Topic) -> Result<Self, Self::Error> {
        value.name.try_into()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum OutgoingTopic {
    SoftwareListRequest,
    SoftwareUpdateRequest,
    SmartRestResponse,
}

impl OutgoingTopic {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::SoftwareListRequest => r#"tedge/commands/req/software/list"#,
            Self::SoftwareUpdateRequest => r#"tedge/commands/req/software/update"#,
            Self::SmartRestResponse => r#"c8y/s/us"#,
        }
    }

    pub(crate) fn to_topic(&self) -> Result<Topic, MqttClientError> {
        match self {
            Self::SoftwareListRequest => Topic::new(Self::SoftwareListRequest.as_str()),
            Self::SoftwareUpdateRequest => Topic::new(Self::SoftwareUpdateRequest.as_str()),
            Self::SmartRestResponse => Topic::new(Self::SmartRestResponse.as_str()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::TryInto;

    #[test]
    fn convert_incoming_topic_to_str() {
        assert_eq!(
            IncomingTopic::SoftwareListResponse.as_str(),
            "tedge/commands/res/software/list"
        );
        assert_eq!(
            IncomingTopic::SoftwareUpdateResponse.as_str(),
            "tedge/commands/res/software/update"
        );
        assert_eq!(IncomingTopic::SmartRestRequest.as_str(), "c8y/s/ds");
    }

    #[test]
    fn convert_str_into_incoming_topic() {
        let list: IncomingTopic = "tedge/commands/res/software/list".try_into().unwrap();
        assert_eq!(list, IncomingTopic::SoftwareListResponse);
        let update: IncomingTopic = "tedge/commands/res/software/update".try_into().unwrap();
        assert_eq!(update, IncomingTopic::SoftwareUpdateResponse);
        let c8y: IncomingTopic = "c8y/s/ds".try_into().unwrap();
        assert_eq!(c8y, IncomingTopic::SmartRestRequest);
        let error: Result<IncomingTopic, MapperTopicError> = "test".try_into();
        assert!(error.is_err());
    }

    #[test]
    fn convert_topic_into_incoming_topic() {
        let list: IncomingTopic = Topic::new("tedge/commands/res/software/list")
            .unwrap()
            .try_into()
            .unwrap();
        assert_eq!(list, IncomingTopic::SoftwareListResponse);
        let update: IncomingTopic = Topic::new("tedge/commands/res/software/update")
            .unwrap()
            .try_into()
            .unwrap();
        assert_eq!(update, IncomingTopic::SoftwareUpdateResponse);
        let c8y: IncomingTopic = Topic::new("c8y/s/ds").unwrap().try_into().unwrap();
        assert_eq!(c8y, IncomingTopic::SmartRestRequest);
        let error: Result<IncomingTopic, MapperTopicError> = Topic::new("test").unwrap().try_into();
        assert!(error.is_err());
    }

    #[test]
    fn convert_outgoing_topic_to_str() {
        assert_eq!(
            OutgoingTopic::SoftwareListRequest.as_str(),
            "tedge/commands/req/software/list"
        );
        assert_eq!(
            OutgoingTopic::SoftwareUpdateRequest.as_str(),
            "tedge/commands/req/software/update"
        );
        assert_eq!(OutgoingTopic::SmartRestResponse.as_str(), "c8y/s/us");
    }

    #[test]
    fn convert_outgoing_topic_to_topic() {
        assert_eq!(
            OutgoingTopic::SoftwareListRequest.to_topic().unwrap(),
            Topic::new("tedge/commands/req/software/list").unwrap()
        );
        assert_eq!(
            OutgoingTopic::SoftwareUpdateRequest.to_topic().unwrap(),
            Topic::new("tedge/commands/req/software/update").unwrap()
        );
        assert_eq!(
            OutgoingTopic::SmartRestResponse.to_topic().unwrap(),
            Topic::new("c8y/s/us").unwrap()
        );
    }
}
