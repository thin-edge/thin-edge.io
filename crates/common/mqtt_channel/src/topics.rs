use crate::errors::MqttError;
use crate::MqttMessage;
use rumqttc::QoS;
use rumqttc::SubscribeFilter;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashSet;
use std::convert::TryInto;
use std::fmt::Display;
use std::fmt::Formatter;

/// An MQTT topic
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct Topic {
    pub name: String,
}

impl Display for Topic {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        Display::fmt(&self.name, f)
    }
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
    patterns: Vec<String>,
    qos: QoS,
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
    pub fn accept(&self, msg: &MqttMessage) -> bool {
        self.accept_topic(&msg.topic)
    }

    /// Simplify the list of patterns, removing overlaps.
    ///
    /// Return the patterns that have been removed.
    pub fn remove_overlapping_patterns(&mut self) -> Vec<String> {
        let mut patterns = vec![];
        let mut removed = vec![];
        patterns.append(&mut self.patterns);

        for pattern in patterns {
            if self.include_topic(&pattern) {
                removed.push(pattern)
            } else {
                let mut sub_patterns = vec![];
                sub_patterns.append(&mut self.patterns);
                for sub_pattern in sub_patterns {
                    if rumqttc::matches(&sub_pattern, &pattern) {
                        removed.push(sub_pattern);
                    } else {
                        self.patterns.push(sub_pattern);
                    }
                }
                self.patterns.push(pattern)
            }
        }

        removed
    }

    /// Check if the given pattern is already matched by this filter pattern.
    fn include_topic(&self, sub_pattern: &str) -> bool {
        self.patterns
            .iter()
            .any(|pattern| rumqttc::matches(sub_pattern, pattern))
    }

    /// A clone topic filter with the given QoS
    pub fn with_qos(self, qos: QoS) -> Self {
        Self { qos, ..self }
    }

    /// The list of `SubscribeFilter` expected by `mqttc`
    pub fn filters(&self) -> Vec<SubscribeFilter> {
        let qos = self.qos;
        self.patterns
            .iter()
            .map(|path| SubscribeFilter {
                path: path.clone(),
                qos,
            })
            .collect()
    }

    pub fn patterns(&self) -> &Vec<String> {
        &self.patterns
    }

    pub fn remove(&mut self, topic: &str) -> Option<String> {
        if let Some((index, _)) = self.patterns.iter().enumerate().find(|(_, p)| *p == topic) {
            Some(self.patterns.swap_remove(index))
        } else {
            None
        }
    }

    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
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

impl FromIterator<TopicFilter> for TopicFilter {
    fn from_iter<T: IntoIterator<Item = TopicFilter>>(filters: T) -> Self {
        let mut combined_filters = TopicFilter::empty();
        for filter in filters.into_iter() {
            combined_filters.add_all(filter)
        }
        combined_filters
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

impl AsRef<str> for Topic {
    fn as_ref(&self) -> &str {
        &self.name
    }
}

impl TryInto<TopicFilter> for HashSet<String> {
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

    #[test]
    fn check_removing_overlapping_patterns() {
        let mut topics = TopicFilter::empty();
        assert!(topics.remove_overlapping_patterns().is_empty());

        // One can adds several patterns, as long as non overlapping
        topics.add_unchecked("te/+/+/+/+/cmd/+/+");
        topics.add_unchecked("te/+/+/+/+/m/+");
        topics.add_unchecked("te/device/main///e/+");
        topics.add_unchecked("te/device/child///e/+");
        assert!(topics.remove_overlapping_patterns().is_empty());

        // If a sub pattern is added, the overlapping is detected
        topics.add_unchecked("te/device/main///m/+");
        let removed = topics.remove_overlapping_patterns();
        assert_eq!(removed.len(), 1);
        assert!(removed.contains(&"te/device/main///m/+".to_string()));

        // If a super pattern is added, the sub patterns are removed
        topics.add_unchecked("te/+/+/+/+/e/+");
        let removed = topics.remove_overlapping_patterns();
        assert_eq!(removed.len(), 2);
        assert!(removed.contains(&"te/device/main///e/+".to_string()));
        assert!(removed.contains(&"te/device/child///e/+".to_string()));

        // Unfortunately, some overlaps are not detected
        // In the following case a message published on `te/xxx/xxx` might be received twice
        topics.add_unchecked("te/xxx/+");
        topics.add_unchecked("te/+/xxx");
        let removed = topics.remove_overlapping_patterns();
        assert!(removed.is_empty());
    }
}
