use crate::json_c8y::C8yAlarm;
use mqtt_channel::MqttError;
use mqtt_channel::Topic;
use mqtt_channel::TopicFilter;
use tedge_api::entity_store::EntityMetadata;
use tedge_api::entity_store::EntityType;
use tedge_api::topic::ResponseTopic;
use tedge_api::TopicError;

pub const SMARTREST_PUBLISH_TOPIC: &str = "c8y/s/us";
pub const SMARTREST_SUBSCRIBE_TOPIC: &str = "c8y/s/ds";

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum C8yTopic {
    SmartRestRequest,
    SmartRestResponse,
    ChildSmartRestResponse(String),
    OperationTopic(String),
}

impl C8yTopic {
    /// Return the c8y SmartRest response topic for the given entity
    pub fn smartrest_response_topic(entity: &EntityMetadata) -> Option<Topic> {
        match entity.r#type {
            EntityType::MainDevice => Some(C8yTopic::upstream_topic()),
            EntityType::ChildDevice | EntityType::Service => {
                Self::ChildSmartRestResponse(entity.external_id.clone().into())
                    .to_topic()
                    .ok()
            }
        }
    }

    pub fn to_topic(&self) -> Result<Topic, MqttError> {
        Topic::new(self.to_string().as_str())
    }

    pub fn upstream_topic() -> Topic {
        Topic::new_unchecked(SMARTREST_PUBLISH_TOPIC)
    }

    pub fn downstream_topic() -> Topic {
        Topic::new_unchecked(SMARTREST_SUBSCRIBE_TOPIC)
    }
}

impl ToString for C8yTopic {
    fn to_string(&self) -> String {
        match self {
            Self::SmartRestRequest => SMARTREST_SUBSCRIBE_TOPIC.into(),
            Self::SmartRestResponse => SMARTREST_PUBLISH_TOPIC.into(),
            Self::ChildSmartRestResponse(child_id) => {
                format!("{}/{}", SMARTREST_PUBLISH_TOPIC, child_id)
            }
            Self::OperationTopic(name) => name.into(),
        }
    }
}

impl TryFrom<String> for C8yTopic {
    type Error = TopicError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            SMARTREST_SUBSCRIBE_TOPIC => Ok(C8yTopic::SmartRestRequest),
            SMARTREST_PUBLISH_TOPIC => Ok(C8yTopic::SmartRestResponse),
            topic_name => {
                let prefix = format!("{}/", SMARTREST_PUBLISH_TOPIC);
                if topic_name.starts_with(&prefix) {
                    Ok(C8yTopic::ChildSmartRestResponse(
                        topic_name.strip_prefix(&prefix).unwrap().into(),
                    ))
                } else if topic_name[..3].contains("c8y") {
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

impl From<C8yTopic> for TopicFilter {
    fn from(val: C8yTopic) -> Self {
        val.to_string().as_str().try_into().expect("infallible")
    }
}

impl From<&C8yAlarm> for C8yTopic {
    fn from(value: &C8yAlarm) -> Self {
        match value {
            C8yAlarm::Create(alarm) => match alarm.source.as_ref() {
                None => Self::SmartRestResponse,
                Some(info) => Self::ChildSmartRestResponse(info.id.clone()),
            },
            C8yAlarm::Clear(alarm) => match alarm.source.as_ref() {
                None => Self::SmartRestResponse,
                Some(info) => Self::ChildSmartRestResponse(info.id.clone()),
            },
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
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

/// Generates the SmartREST topic to publish to, for a given managed object
/// from the list of external IDs of itself and all its parents.
///
/// The parents are appended in the reverse order,
/// starting from the main device at the end of the list.
/// The main device itself is represented by the root topic c8y/s/us,
/// with the rest of the children appended to it at each topic level.
///
/// # Examples
///
/// - `["main"]` -> `c8y/s/us`
/// - `["child1", "main"]` -> `c8y/s/us/child1`
/// - `["child2", "child1", "main"]` -> `c8y/s/us/child1/child2`
pub fn publish_topic_from_ancestors(ancestors: &[String]) -> Topic {
    let mut target_topic = SMARTREST_PUBLISH_TOPIC.to_string();
    for ancestor in ancestors.iter().rev().skip(1) {
        // Skipping the last ancestor as it is the main device represented by the root topic itself
        target_topic.push('/');
        target_topic.push_str(ancestor);
    }

    Topic::new_unchecked(&target_topic)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::TryInto;
    use test_case::test_case;

    #[test]
    fn convert_c8y_topic_to_str() {
        assert_eq!(&C8yTopic::SmartRestRequest.to_string(), "c8y/s/ds");
        assert_eq!(&C8yTopic::SmartRestResponse.to_string(), "c8y/s/us");
        assert_eq!(
            &C8yTopic::ChildSmartRestResponse("child-id".into()).to_string(),
            "c8y/s/us/child-id"
        );
    }

    #[test]
    fn convert_str_into_c8y_topic() {
        let c8y_req: C8yTopic = "c8y/s/ds".try_into().unwrap();
        assert_eq!(c8y_req, C8yTopic::SmartRestRequest);
        let c8y_resp: C8yTopic = "c8y/s/us".try_into().unwrap();
        assert_eq!(c8y_resp, C8yTopic::SmartRestResponse);
        let c8y_resp: C8yTopic = "c8y/s/us/child-id".try_into().unwrap();
        assert_eq!(
            c8y_resp,
            C8yTopic::ChildSmartRestResponse("child-id".into())
        );
        let error: Result<C8yTopic, TopicError> = "test".try_into();
        assert!(error.is_err());
    }

    #[test]
    fn convert_topic_into_c8y_topic() {
        let c8y_req: C8yTopic = Topic::new("c8y/s/ds").unwrap().try_into().unwrap();
        assert_eq!(c8y_req, C8yTopic::SmartRestRequest);

        let c8y_resp: C8yTopic = Topic::new("c8y/s/us").unwrap().try_into().unwrap();
        assert_eq!(c8y_resp, C8yTopic::SmartRestResponse);

        let c8y_resp: C8yTopic = Topic::new("c8y/s/us/child-id").unwrap().try_into().unwrap();
        assert_eq!(
            c8y_resp,
            C8yTopic::ChildSmartRestResponse("child-id".into())
        );

        let error: Result<C8yTopic, TopicError> = Topic::new("test").unwrap().try_into();
        assert!(error.is_err());
    }

    #[test_case(&["main"], "c8y/s/us")]
    #[test_case(&["foo"], "c8y/s/us")]
    #[test_case(&["child1", "main"], "c8y/s/us/child1")]
    #[test_case(&["child3", "child2", "child1", "main"], "c8y/s/us/child1/child2/child3")]
    fn topic_from_ancestors(ancestors: &[&str], topic: &str) {
        let ancestors: Vec<String> = ancestors.iter().map(|v| v.to_string()).collect();
        let nested_child_topic = publish_topic_from_ancestors(&ancestors);
        assert_eq!(nested_child_topic, Topic::new_unchecked(topic));
    }
}
