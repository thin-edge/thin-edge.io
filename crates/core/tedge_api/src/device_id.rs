// TODO: Has to extract this from configuration
const ROOT_PREFIX: &str = "te";
/// This creates the child device external ID prefixing the main device name (device_common_name), device and child device name separated by `:`
/// For example `device_common_name:device:child_device_name`
pub fn get_external_identity_from_topic(
    device_common_name: String,
    topic: String,
) -> Option<String> {
    if topic.starts_with(&format!("{ROOT_PREFIX}/device")) {
        let topic_split = topic.split('/').collect::<Vec<_>>();
        if topic_split.len() >= 5 {
            if topic_split[2].eq("main") && topic_split[3].ne("service") {
                return None;
            }

            let mut child_device_name = device_common_name;
            for item in topic_split.iter().take(5) {
                if item.ne(&ROOT_PREFIX) && !item.is_empty() && item.ne(&"main"){
                    child_device_name.push_str(&format!(":{}", item));
                }
            }
            Some(child_device_name)
        } else {
            None
        }
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;
    #[test_case("test_device".to_string(), "te/device/main///m/".to_string(), None; "main device id")]
    #[test_case("test_device".to_string(), "te/device/child///m/".to_string(), Some("test_device:device:child".to_string()); "child device id")]
    #[test_case("test_device".to_string(), "te/device/child/a1/a2/m/".to_string(), Some("test_device:device:child:a1:a2".to_string()); "child with more components device id")]
    #[test_case("test_device".to_string(), "te/device/main/service/nodered/m/".to_string(), Some("test_device:device:service:nodered".to_string()); "service id")]
    #[test_case("test_device".to_string(), "foo/bar".to_string(), None; "wrong topic")]
    fn child_device_id(parent_id: String, topic: String, expected_child_device_id: Option<String>) {
        let actual_device_id = get_external_identity_from_topic(parent_id, topic);
        assert_eq!(actual_device_id, expected_child_device_id);
    }
}
