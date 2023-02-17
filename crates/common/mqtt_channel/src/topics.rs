use crate::errors::MqttError;
use crate::Message;
use rumqttc::QoS;
use rumqttc::SubscribeFilter;
use std::convert::TryInto;

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

    /// Build a new topic, assuming the name is valid
    pub fn new_unchecked(name: &str) -> Topic {
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

impl Default for TopicFilter {
    fn default() -> Self {
        TopicFilter::empty()
    }
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

    /// An empty topic filter
    pub fn empty() -> TopicFilter {
        TopicFilter {
            patterns: vec![],
            qos: QoS::AtLeastOnce,
        }
    }

    /// Build a new topic filter, assuming the pattern is valid.
    pub fn new_unchecked(pattern: &str) -> TopicFilter {
        let patterns = vec![String::from(pattern)];
        TopicFilter {
            patterns,
            qos: QoS::AtLeastOnce,
        }
    }

    /// Check if the pattern is valid and add it to this topic filter.
    pub fn add(&mut self, pattern: &str) -> Result<(), MqttError> {
        let pattern = String::from(pattern);
        if rumqttc::valid_filter(&pattern) {
            self.patterns.push(pattern);
            Ok(())
        } else {
            Err(MqttError::InvalidFilter { pattern })
        }
    }

    /// Assuming the pattern is valid and add it to this topic filter.
    pub fn add_unchecked(&mut self, pattern: &str) {
        let pattern = String::from(pattern);
        self.patterns.push(pattern);
    }

    /// Add all the other topics to this one.
    pub fn add_all(&mut self, other: TopicFilter) {
        for pattern in other.patterns {
            self.patterns.push(pattern)
        }
    }

    /// Check if the given topic matches this filter pattern.
    pub fn accept_topic(&self, topic: &Topic) -> bool {
        self.patterns
            .iter()
            .any(|pattern| rumqttc::matches(&topic.name, pattern))
    }

    /// Check if the given message matches this filter pattern.
    pub fn accept(&self, msg: &Message) -> bool {
        self.accept_topic(&msg.topic)
    }

    /// A clone topic filter with the given QoS
    pub fn with_qos(self, qos: QoS) -> Self {
        Self { qos, ..self }
    }

    /// The list of `SubscribeFilter` expected by `mqttc`
    pub(crate) fn filters(&self) -> Vec<SubscribeFilter> {
        let qos = self.qos;
        self.patterns
            .iter()
            .map(|path| SubscribeFilter {
                path: path.clone(),
                qos,
            })
            .collect()
    }
}

impl TryInto<Topic> for &str {
    type Error = MqttError;

    fn try_into(self) -> Result<Topic, Self::Error> {
        Topic::new(self)
    }
}

impl From<Topic> for TopicFilter {
    fn from(topic: Topic) -> TopicFilter {
        topic.filter()
    }
}

impl TryInto<TopicFilter> for &str {
    type Error = MqttError;

    fn try_into(self) -> Result<TopicFilter, Self::Error> {
        TopicFilter::new(self)
    }
}

impl TryInto<TopicFilter> for Vec<&str> {
    type Error = MqttError;

    fn try_into(self) -> Result<TopicFilter, Self::Error> {
        let mut filter = TopicFilter::empty();
        for pattern in self.into_iter() {
            filter.add(pattern)?
        }
        Ok(filter)
    }
}

impl From<Topic> for String {
    fn from(topic: Topic) -> Self {
        topic.name
    }
}

impl TryInto<TopicFilter> for Vec<String> {
    type Error = MqttError;

    fn try_into(self) -> Result<TopicFilter, Self::Error> {
        let mut filter = TopicFilter::empty();
        for pattern in self.into_iter() {
            filter.add(pattern.as_str())?
        }
        Ok(filter)
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
