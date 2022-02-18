use agent_interface::topic::ResponseTopic;
use agent_interface::TopicError;
use mqtt_channel::MqttError;
use mqtt_channel::Topic;

#[derive(Debug, Clone, PartialEq)]
pub enum C8yTopic {
    SmartRestRequest,
    SmartRestResponse,
    OperationTopic(String),
}

impl C8yTopic {
    pub fn as_str(&self) -> &str {
        match self {
            Self::SmartRestRequest => r#"c8y/s/ds"#,
            Self::SmartRestResponse => r#"c8y/s/us"#,
            Self::OperationTopic(name) => name.as_str(),
        }
    }

    pub fn to_topic(&self) -> Result<Topic, MqttError> {
        Topic::new(self.as_str())
    }
}

impl TryFrom<String> for C8yTopic {
    type Error = TopicError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            r#"c8y/s/ds"# => Ok(C8yTopic::SmartRestRequest),
            r#"c8y/s/us"# => Ok(C8yTopic::SmartRestResponse),
            topic_name => {
                if topic_name[..3].contains("c8y") {
                    Ok(C8yTopic::OperationTopic(topic_name.to_string()))
                } else {
                    Err(TopicError::UnknownTopic {
                        topic: topic_name.to_string(),
                    })
                }
            }
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
    C8yTopic(C8yTopic),
    ResponseTopic(ResponseTopic),
}

impl TryFrom<String> for MapperSubscribeTopic {
    type Error = TopicError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        match ResponseTopic::try_from(value.clone()) {
            Ok(response_topic) => Ok(MapperSubscribeTopic::ResponseTopic(response_topic)),
            Err(_) => match C8yTopic::try_from(value) {
                Ok(smart_rest_request) => Ok(MapperSubscribeTopic::C8yTopic(smart_rest_request)),
                Err(err) => Err(err),
            },
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
