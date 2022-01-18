use crate::error::TopicError;
use std::convert::TryFrom;
#[derive(Debug, Clone, PartialEq)]
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

#[derive(Debug, Clone, PartialEq)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::TryInto;

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
}
