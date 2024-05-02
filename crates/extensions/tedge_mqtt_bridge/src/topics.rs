use crate::BridgeRule;
use rumqttc::matches;
use std::borrow::Cow;

pub fn prepend<'a>(target: Cow<'a, str>, prefix: &Cow<'a, str>) -> Cow<'a, str> {
    match (prefix, target) {
        (prefix, target) if prefix.is_empty() => target,
        (prefix, target) if target.is_empty() => prefix.clone(),
        (prefix, Cow::Borrowed(target)) => format!("{prefix}{target}").into(),
        (prefix, Cow::Owned(mut target)) => {
            target.insert_str(0, prefix.as_ref());
            Cow::Owned(target)
        }
    }
}

pub fn matches_ignore_dollar_prefix(topic: &str, filter: &str) -> bool {
    match (&topic[..1], &filter[..1]) {
        ("$", "$") => matches(&topic[1..], &filter[1..]),
        _ => matches(topic, filter),
    }
}

pub struct TopicConverter(pub Vec<BridgeRule>);

impl TopicConverter {
    pub fn convert_topic<'a>(&'a self, topic: &'a str) -> Cow<'a, str> {
        self.0
            .iter()
            .find_map(|rule| rule.apply(topic))
            // TODO should this be an error
            .unwrap_or_else(|| panic!("Failed to convert {topic:?} with {:?}", self.0))
    }
}
