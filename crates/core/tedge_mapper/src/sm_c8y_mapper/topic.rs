use agent_interface::{error::*, topic::ResponseTopic};
use mqtt_client::{MqttClientError, Topic};
use std::convert::{TryFrom, TryInto};

#[derive(Debug, Clone, PartialEq)]
pub enum C8yTopic {
    SmartRestRequest,
    SmartRestResponse,
}

impl C8yTopic {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SmartRestRequest => r#"c8y/s/ds"#,
            Self::SmartRestResponse => r#"c8y/s/us"#,
        }
    }

    pub fn to_topic(&self) -> Result<Topic, MqttClientError> {
        match self {
            Self::SmartRestRequest => Topic::new(Self::SmartRestRequest.as_str()),
            Self::SmartRestResponse => Topic::new(Self::SmartRestResponse.as_str()),
        }
    }
}

impl TryFrom<String> for C8yTopic {
    type Error = TopicError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            r#"c8y/s/ds"# => Ok(C8yTopic::SmartRestRequest),
            r#"c8y/s/us"# => Ok(C8yTopic::SmartRestResponse),
            err => Err(TopicError::UnknownTopic {
                topic: err.to_string(),
            }),
        }
    }
}
impl TryFrom<&str> for C8yTopic {
    type Error = TopicError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::try_from(value.to_string())
    }
}

impl TryFrom<Topic> for C8yTopic {
    type Error = TopicError;

    fn try_from(value: Topic) -> Result<Self, Self::Error> {
        value.name.try_into()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum MapperSubscribeTopic {
    SmartRestRequest,
    ResponseTopic(ResponseTopic),
}

impl TryFrom<String> for MapperSubscribeTopic {
    type Error = TopicError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            r#"c8y/s/ds"# => Ok(MapperSubscribeTopic::SmartRestRequest),
            r#"tedge/commands/res/software/list"# => Ok(MapperSubscribeTopic::ResponseTopic(
                ResponseTopic::SoftwareListResponse,
            )),
            r#"tedge/commands/res/software/update"# => Ok(MapperSubscribeTopic::ResponseTopic(
                ResponseTopic::SoftwareUpdateResponse,
            )),
            r#"tedge/commands/res/control/restart"# => Ok(MapperSubscribeTopic::ResponseTopic(
                ResponseTopic::RestartResponse,
            )),
            err => Err(TopicError::UnknownTopic {
                topic: err.to_string(),
            }),
        }
    }
}

impl TryFrom<&str> for MapperSubscribeTopic {
    type Error = TopicError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::try_from(value.to_string())
    }
}

impl TryFrom<Topic> for MapperSubscribeTopic {
    type Error = TopicError;

    fn try_from(value: Topic) -> Result<Self, Self::Error> {
        value.name.try_into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::TryInto;

    #[test]
    fn convert_c8y_topic_to_str() {
        assert_eq!(C8yTopic::SmartRestRequest.as_str(), "c8y/s/ds");
        assert_eq!(C8yTopic::SmartRestResponse.as_str(), "c8y/s/us");
    }

    #[test]
    fn convert_str_into_c8y_topic() {
        let c8y_req: C8yTopic = "c8y/s/ds".try_into().unwrap();
        assert_eq!(c8y_req, C8yTopic::SmartRestRequest);
        let c8y_resp: C8yTopic = "c8y/s/us".try_into().unwrap();
        assert_eq!(c8y_resp, C8yTopic::SmartRestResponse);
        let error: Result<C8yTopic, TopicError> = "test".try_into();
        assert!(error.is_err());
    }

    #[test]
    fn convert_topic_into_c8y_topic() {
        let c8y_req: C8yTopic = Topic::new("c8y/s/ds").unwrap().try_into().unwrap();
        assert_eq!(c8y_req, C8yTopic::SmartRestRequest);

        let c8y_resp: C8yTopic = Topic::new("c8y/s/us").unwrap().try_into().unwrap();
        assert_eq!(c8y_resp, C8yTopic::SmartRestResponse);
        let error: Result<C8yTopic, TopicError> = Topic::new("test").unwrap().try_into();
        assert!(error.is_err());
    }
}
