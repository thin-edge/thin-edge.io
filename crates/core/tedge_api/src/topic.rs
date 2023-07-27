use crate::error::TopicError;
use std::convert::TryFrom;
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ResponseTopic {
    SoftwareListResponse,
    SoftwareUpdateResponse,
    RestartResponse,
}

impl ResponseTopic {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SoftwareListResponse => r#"tedge/commands/res/software/list"#,
            Self::SoftwareUpdateResponse => r#"tedge/commands/res/software/update"#,
            Self::RestartResponse => r#"tedge/commands/res/control/restart"#,
        }
    }
}

impl TryFrom<String> for ResponseTopic {
    type Error = TopicError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            r#"tedge/commands/res/software/list"# => Ok(ResponseTopic::SoftwareListResponse),
            r#"tedge/commands/res/software/update"# => Ok(ResponseTopic::SoftwareUpdateResponse),
            r#"tedge/commands/res/control/restart"# => Ok(ResponseTopic::RestartResponse),
            err => Err(TopicError::UnknownTopic {
                topic: err.to_string(),
            }),
        }
    }
}

impl TryFrom<&str> for ResponseTopic {
    type Error = TopicError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::try_from(value.to_string())
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum RequestTopic {
    SoftwareListRequest,
    SoftwareUpdateRequest,
    RestartRequest,
}

impl RequestTopic {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SoftwareListRequest => r#"tedge/commands/req/software/list"#,
            Self::SoftwareUpdateRequest => r#"tedge/commands/req/software/update"#,
            Self::RestartRequest => r#"tedge/commands/req/control/restart"#,
        }
    }
}

pub fn get_child_id_from_measurement_topic(topic: &str) -> Option<String> {
    match topic.strip_prefix("tedge/measurements/").map(String::from) {
        Some(maybe_id) if maybe_id.is_empty() => None,
        option => option,
    }
}

pub fn get_child_id_from_child_topic(topic: &str) -> Option<String> {
    let mut topic_split = topic.split('/');
    // the second element is the child id
    topic_split.nth(1).and_then(|id| {
        if id.is_empty() {
            None
        } else {
            Some(id.to_string())
        }
    })
}

#[cfg(test)]
mod tests {

    use super::*;
    use std::convert::TryInto;
    use test_case::test_case;

    #[test]
    fn convert_response_topic_to_str() {
        assert_eq!(
            ResponseTopic::SoftwareListResponse.as_str(),
            "tedge/commands/res/software/list"
        );
        assert_eq!(
            ResponseTopic::SoftwareUpdateResponse.as_str(),
            "tedge/commands/res/software/update"
        );
    }

    #[test]
    fn convert_str_into_response_topic() {
        let list: ResponseTopic = "tedge/commands/res/software/list".try_into().unwrap();
        assert_eq!(list, ResponseTopic::SoftwareListResponse);
        let update: ResponseTopic = "tedge/commands/res/software/update".try_into().unwrap();
        assert_eq!(update, ResponseTopic::SoftwareUpdateResponse);

        let error: Result<ResponseTopic, TopicError> = "test".try_into();
        assert!(error.is_err());
    }

    #[test]
    fn convert_request_topic_to_str() {
        assert_eq!(
            RequestTopic::SoftwareListRequest.as_str(),
            "tedge/commands/req/software/list"
        );
        assert_eq!(
            RequestTopic::SoftwareUpdateRequest.as_str(),
            "tedge/commands/req/software/update"
        );
    }

    #[test_case("tedge/measurements/test", Some("test"); "valid child id")]
    #[test_case("tedge/measurements/", None; "invalid child id (empty value)")]
    #[test_case("tedge/measurements", None; "invalid child id (parent topic)")]
    #[test_case("foo/bar", None; "invalid child id (invalid topic)")]
    fn extract_child_id_from_measurement_topic(in_topic: &str, expected_child_id: Option<&str>) {
        assert_eq!(
            get_child_id_from_measurement_topic(in_topic),
            expected_child_id.map(|s| s.to_string())
        );
    }

    #[test_case("tedge/child1/commands/firmware/update", Some("child1"); "valid child id")]
    #[test_case("tedge/", None; "invalid child id 1")]
    #[test_case("tedge//commands/firmware/update", None; "invalid child id 2")]
    #[test_case("tedge", None; "invalid child id 3")]
    fn extract_child_id(in_topic: &str, expected_child_id: Option<&str>) {
        assert_eq!(
            get_child_id_from_child_topic(in_topic),
            expected_child_id.map(|s| s.to_string())
        );
    }
}
