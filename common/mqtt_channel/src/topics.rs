use rumqttc::QoS;
use crate::errors::MqttError;

/// An MQTT topic
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Topic {
    pub name: String,
}

impl Topic {
    /// Check if the topic name is valid and build a new topic.
    pub fn new(name: &str) -> Result<Topic, MqttError> {
        let name = String::from(name);
        if rumqttc::valid_topic(&name) {
            Ok(Topic { name })
        } else {
            Err(MqttError::InvalidTopic { name })
        }
    }

    /// Build a new topic, assuming the name is valid since received from mqtt.
    fn incoming(name: &str) -> Topic {
        let name = String::from(name);
        Topic { name }
    }

    /// Build a topic filter filtering only that topic
    pub fn filter(&self) -> TopicFilter {
        TopicFilter {
            patterns: vec![self.name.clone()],
            qos: QoS::AtLeastOnce,
        }
    }
}

/// An MQTT topic filter
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TopicFilter {
    pub patterns: Vec<String>,
    pub qos: QoS,
}

impl TopicFilter {
    /// Check if the pattern is valid and build a new topic filter.
    pub fn new(pattern: &str) -> Result<TopicFilter, MqttError> {
        let pattern = String::from(pattern);
        let qos = QoS::AtLeastOnce;
        if rumqttc::valid_filter(&pattern) {
            Ok(TopicFilter {
                patterns: vec![pattern],
                qos,
            })
        } else {
            Err(MqttError::InvalidFilter { pattern })
        }
    }

    /// Check if the pattern is valid and at it to this topic filter.
    pub fn add(&mut self, pattern: &str) -> Result<(), MqttError> {
        let pattern = String::from(pattern);
        if rumqttc::valid_filter(&pattern) {
            self.patterns.push(pattern);
            Ok(())
        } else {
            Err(MqttError::InvalidFilter { pattern })
        }
    }

    /// Check if the given topic matches this filter pattern.
    fn accept(&self, topic: &Topic) -> bool {
        self.patterns
            .iter()
            .any(|pattern| rumqttc::matches(&topic.name, pattern))
    }

    /// A clone topic filter with the given QoS
    pub fn with_qos(self, qos: QoS) -> Self {
        Self { qos, ..self }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_valid_topic() {
        assert!(Topic::new("temp").is_ok());
        assert!(Topic::new("temp/device-12").is_ok());
    }

    #[test]
    fn check_invalid_topic() {
        assert!(Topic::new("/temp/+").is_err());
        assert!(Topic::new("/temp/#").is_err());
    }

    #[test]
    fn check_valid_topic_filter() {
        assert!(TopicFilter::new("a/b/c").is_ok());
        assert!(TopicFilter::new("a/b/#").is_ok());
        assert!(TopicFilter::new("a/b/+").is_ok());
        assert!(TopicFilter::new("a/+/b").is_ok());
    }

    #[test]
    fn check_invalid_topic_filter() {
        assert!(TopicFilter::new("").is_err());
        assert!(TopicFilter::new("/a/#/b").is_err());
        assert!(TopicFilter::new("/a/#/+").is_err());
    }
}
