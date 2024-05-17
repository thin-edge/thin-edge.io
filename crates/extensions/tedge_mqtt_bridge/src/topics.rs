use crate::BridgeRule;
use rumqttc::matches;
use std::borrow::Cow;
use tracing::log::warn;

pub fn matches_ignore_dollar_prefix(topic: &str, filter: &str) -> bool {
    match (&topic[..1], &filter[..1]) {
        ("$", "$") => matches(&topic[1..], &filter[1..]),
        _ => matches(topic, filter),
    }
}

pub struct TopicConverter(pub Vec<BridgeRule>);

impl TopicConverter {
    pub fn convert_topic<'a>(&'a self, topic: &'a str) -> Option<Cow<'a, str>> {
        self.0
            .iter()
            .find_map(|rule| rule.apply(topic))
            .or_else(|| {
                warn!("Failed to convert {topic:?}");
                None
            })
    }
}
