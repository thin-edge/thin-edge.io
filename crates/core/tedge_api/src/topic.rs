// FIXME Deprecate this method
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
    use test_case::test_case;

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
