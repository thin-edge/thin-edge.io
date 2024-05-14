use crate::json_c8y::C8yAlarm;
use mqtt_channel::MqttError;
use mqtt_channel::Topic;
use mqtt_channel::TopicFilter;
use tedge_api::entity_store::EntityMetadata;
use tedge_api::entity_store::EntityType;
use tedge_config::TopicPrefix;

const SMARTREST_PUBLISH_TOPIC: &str = "s/us";
const SMARTREST_SUBSCRIBE_TOPIC: &str = "s/ds";

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum C8yTopic {
    SmartRestRequest,
    SmartRestResponse,
    ChildSmartRestResponse(String),
}

impl C8yTopic {
    /// Return the c8y SmartRest response topic for the given entity
    pub fn smartrest_response_topic(
        entity: &EntityMetadata,
        prefix: &TopicPrefix,
    ) -> Option<Topic> {
        match entity.r#type {
            EntityType::MainDevice => Some(C8yTopic::upstream_topic(prefix)),
            EntityType::ChildDevice | EntityType::Service => {
                Self::ChildSmartRestResponse(entity.external_id.clone().into())
                    .to_topic(prefix)
                    .ok()
            }
        }
    }

    pub fn to_topic(&self, prefix: &TopicPrefix) -> Result<Topic, MqttError> {
        Topic::new(self.with_prefix(prefix).as_str())
    }

    pub fn upstream_topic(prefix: &TopicPrefix) -> Topic {
        Topic::new_unchecked(&Self::SmartRestResponse.with_prefix(prefix))
    }

    pub fn downstream_topic(prefix: &TopicPrefix) -> Topic {
        Topic::new_unchecked(&Self::SmartRestRequest.with_prefix(prefix))
    }

    pub fn with_prefix(&self, prefix: &TopicPrefix) -> String {
        match self {
            Self::SmartRestRequest => format!("{prefix}/{SMARTREST_SUBSCRIBE_TOPIC}"),
            Self::SmartRestResponse => format!("{prefix}/{SMARTREST_PUBLISH_TOPIC}"),
            Self::ChildSmartRestResponse(child_id) => {
                format!("{prefix}/{SMARTREST_PUBLISH_TOPIC}/{child_id}")
            }
        }
    }

    pub fn to_topic_filter(&self, prefix: &TopicPrefix) -> TopicFilter {
        self.with_prefix(prefix)
            .as_str()
            .try_into()
            .expect("infallible")
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
pub fn publish_topic_from_ancestors(ancestors: &[impl AsRef<str>], prefix: &TopicPrefix) -> Topic {
    let mut target_topic = format!("{prefix}/{SMARTREST_PUBLISH_TOPIC}");
    for ancestor in ancestors.iter().rev().skip(1) {
        // Skipping the last ancestor as it is the main device represented by the root topic itself
        target_topic.push('/');
        target_topic.push_str(ancestor.as_ref());
    }

    Topic::new_unchecked(&target_topic)
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[test]
    fn convert_c8y_topic_to_str() {
        assert_eq!(
            &C8yTopic::SmartRestRequest.with_prefix(&"c8y".try_into().unwrap()),
            "c8y/s/ds"
        );
        assert_eq!(
            &C8yTopic::SmartRestResponse.with_prefix(&"c8y".try_into().unwrap()),
            "c8y/s/us"
        );
        assert_eq!(
            &C8yTopic::ChildSmartRestResponse("child-id".into())
                .with_prefix(&"custom".try_into().unwrap()),
            "custom/s/us/child-id"
        );
    }

    #[test]
    fn topic_methods() {
        assert_eq!(
            C8yTopic::upstream_topic(&"c8y-local".try_into().unwrap()),
            Topic::new_unchecked("c8y-local/s/us")
        );
        assert_eq!(
            C8yTopic::downstream_topic(&"custom-topic".try_into().unwrap()),
            Topic::new_unchecked("custom-topic/s/ds")
        )
    }

    #[test_case(& ["main"], "c8y2/s/us")]
    #[test_case(& ["foo"], "c8y2/s/us")]
    #[test_case(& ["child1", "main"], "c8y2/s/us/child1")]
    #[test_case(& ["child3", "child2", "child1", "main"], "c8y2/s/us/child1/child2/child3")]
    fn topic_from_ancestors(ancestors: &[&str], topic: &str) {
        let nested_child_topic =
            publish_topic_from_ancestors(ancestors, &"c8y2".try_into().unwrap());
        assert_eq!(nested_child_topic, Topic::new_unchecked(topic));
    }
}
