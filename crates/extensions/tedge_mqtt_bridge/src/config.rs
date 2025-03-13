use crate::topics::matches_ignore_dollar_prefix;
use crate::topics::TopicConverter;
use certificate::parse_root_certificate::create_tls_config;
use certificate::parse_root_certificate::create_tls_config_without_client_cert;
use rumqttc::valid_filter;
use rumqttc::valid_topic;
use rumqttc::MqttOptions;
use rumqttc::Transport;
use std::borrow::Cow;
use std::path::Path;
use tedge_config::tedge_toml::CloudConfig;

pub fn use_key_and_cert(
    config: &mut MqttOptions,
    cloud_config: &dyn CloudConfig,
) -> anyhow::Result<()> {
    let tls_config = create_tls_config(
        cloud_config.root_cert_path(),
        cloud_config.device_key_path(),
        cloud_config.device_cert_path(),
    )?;
    config.set_transport(Transport::tls_with_config(tls_config.into()));
    Ok(())
}

pub fn use_credentials(
    config: &mut MqttOptions,
    root_cert_path: impl AsRef<Path>,
    username: String,
    password: String,
) -> anyhow::Result<()> {
    let tls_config = create_tls_config_without_client_cert(root_cert_path)?;
    config.set_transport(Transport::tls_with_config(tls_config.into()));
    config.set_credentials(username, password);
    Ok(())
}

#[derive(Default, Debug, Clone)]
pub struct BridgeConfig {
    local_to_remote: Vec<BridgeRule>,
    remote_to_local: Vec<BridgeRule>,
    bidirectional_topics: Vec<(Cow<'static, str>, Cow<'static, str>)>,
}

#[derive(Debug, Clone)]
/// A rule for forwarding MQTT messages from one broker to another
///
/// A rule has three parts, a filter, a prefix to add and a prefix to remove. For instance, the rule
/// `filter: "s/us", prefix_to_remove: "c8y/", prefix_to_add: ""`, will map the topic `c8y/s/us`
/// to `s/us`. The filter can contain wildcards, or be empty (in which case the prefix to remove is
/// the sole local topic that will be remapped).
///
/// ```
/// use tedge_mqtt_bridge::BridgeRule;
///
/// let outgoing_c8y = BridgeRule::try_new("s/us".into(), "c8y/".into(), "".into()).unwrap();
/// assert_eq!(outgoing_c8y.apply("c8y/s/us").unwrap(), "s/us");
///
/// let single_topic = BridgeRule::try_new("".into(), "my/input/topic".into(), "different/output/topic".into()).unwrap();
/// assert_eq!(single_topic.apply("my/input/topic").unwrap(), "different/output/topic");
///
/// let wildcard = BridgeRule::try_new("test/#".into(), "a/".into(), "b/".into()).unwrap();
/// assert_eq!(wildcard.apply("a/test/me").unwrap(), "b/test/me");
/// ```
///
/// This bridge rule logic is based on mosquitto's rule (see the `topic` section of the
/// [mosquitto.conf man page](https://mosquitto.org/man/mosquitto-conf-5.html) for details on what
/// is supported).
pub struct BridgeRule {
    topic_filter: Cow<'static, str>,
    prefix_to_remove: Cow<'static, str>,
    prefix_to_add: Cow<'static, str>,
}

#[derive(Debug, thiserror::Error)]
pub enum InvalidBridgeRule {
    #[error("{0:?} is not a valid MQTT bridge topic prefix as it is missing a trailing slash")]
    MissingTrailingSlash(Cow<'static, str>),

    #[error(
    "{0:?} is not a valid rule, at least one of the topic filter or both prefixes must be non-empty"
    )]
    Empty(BridgeRule),

    #[error("{0:?} is not a valid MQTT bridge topic prefix because it contains '+' or '#'")]
    InvalidTopicPrefix(String),

    #[error("{0:?} is not a valid MQTT bridge topic filter")]
    InvalidTopicFilter(String),
}

fn validate_topic(topic: &str) -> Result<(), InvalidBridgeRule> {
    match valid_topic(topic) {
        true => Ok(()),
        false => Err(InvalidBridgeRule::InvalidTopicPrefix(topic.to_owned())),
    }
}

fn validate_filter(topic: &str) -> Result<(), InvalidBridgeRule> {
    match valid_filter(topic) {
        true => Ok(()),
        false => Err(InvalidBridgeRule::InvalidTopicFilter(topic.to_owned())),
    }
}

impl BridgeRule {
    pub fn try_new(
        base_topic_filter: Cow<'static, str>,
        prefix_to_remove: Cow<'static, str>,
        prefix_to_add: Cow<'static, str>,
    ) -> Result<Self, InvalidBridgeRule> {
        let filter_is_empty = base_topic_filter.is_empty();
        let mut r = Self {
            topic_filter: prefix_to_remove.clone() + base_topic_filter.clone(),
            prefix_to_remove,
            prefix_to_add,
        };

        validate_topic(&r.prefix_to_add)?;
        validate_topic(&r.prefix_to_remove)?;
        if filter_is_empty {
            if r.prefix_to_add.is_empty() || r.prefix_to_remove.is_empty() {
                r.topic_filter = base_topic_filter;
                Err(InvalidBridgeRule::Empty(r))
            } else {
                Ok(r)
            }
        } else if !(r.prefix_to_remove.ends_with('/') || r.prefix_to_remove.is_empty()) {
            Err(InvalidBridgeRule::MissingTrailingSlash(r.prefix_to_remove))
        } else if !(r.prefix_to_add.ends_with('/') || r.prefix_to_add.is_empty()) {
            Err(InvalidBridgeRule::MissingTrailingSlash(r.prefix_to_add))
        } else {
            validate_filter(&base_topic_filter)?;
            Ok(r)
        }
    }

    pub fn apply<'a>(&self, topic: &'a str) -> Option<Cow<'a, str>> {
        matches_ignore_dollar_prefix(topic, &self.topic_filter).then(|| {
            self.prefix_to_add.clone() + topic.strip_prefix(&*self.prefix_to_remove).unwrap()
        })
    }
}

impl BridgeConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn forward_from_local(
        &mut self,
        topic: impl Into<Cow<'static, str>>,
        local_prefix: impl Into<Cow<'static, str>>,
        remote_prefix: impl Into<Cow<'static, str>>,
    ) -> Result<(), InvalidBridgeRule> {
        self.local_to_remote.push(BridgeRule::try_new(
            topic.into(),
            local_prefix.into(),
            remote_prefix.into(),
        )?);
        Ok(())
    }

    pub fn forward_from_remote(
        &mut self,
        topic: impl Into<Cow<'static, str>>,
        local_prefix: impl Into<Cow<'static, str>>,
        remote_prefix: impl Into<Cow<'static, str>>,
    ) -> Result<(), InvalidBridgeRule> {
        self.remote_to_local.push(BridgeRule::try_new(
            topic.into(),
            remote_prefix.into(),
            local_prefix.into(),
        )?);
        Ok(())
    }

    /// Forwards the message in both directions, ensuring that an infinite loop is avoided
    ///
    /// Because this method keeps track of the topic so we don't create an infinite loop of messages
    /// this is not equivalent to calling [forward_from_local] and [forward_from_remote] in sequence.
    pub fn forward_bidirectionally(
        &mut self,
        topic: impl Into<Cow<'static, str>>,
        local_prefix: impl Into<Cow<'static, str>>,
        remote_prefix: impl Into<Cow<'static, str>>,
    ) -> Result<(), InvalidBridgeRule> {
        let topic = topic.into();
        let local_prefix = local_prefix.into();
        let remote_prefix = remote_prefix.into();
        self.bidirectional_topics.push((
            local_prefix.clone() + topic.clone(),
            remote_prefix.clone() + topic.clone(),
        ));
        self.forward_from_local(topic.clone(), local_prefix.clone(), remote_prefix.clone())?;
        self.forward_from_remote(topic, local_prefix, remote_prefix)?;
        Ok(())
    }

    pub fn local_subscriptions(&self) -> impl Iterator<Item = &str> {
        self.local_to_remote
            .iter()
            .map(|rule| rule.topic_filter.as_ref())
    }

    pub fn remote_subscriptions(&self) -> impl Iterator<Item = &str> {
        self.remote_to_local.iter().map(|rule| &*rule.topic_filter)
    }

    pub(super) fn converters_and_bidirectional_topic_filters(
        self,
    ) -> [(TopicConverter, Vec<Cow<'static, str>>); 2] {
        let Self {
            local_to_remote,
            remote_to_local,
            bidirectional_topics,
        } = self;

        let (bidir_local_topics, bidir_remote_topics) = bidirectional_topics.into_iter().unzip();
        [
            (TopicConverter(local_to_remote), bidir_local_topics),
            (TopicConverter(remote_to_local), bidir_remote_topics),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod use_key_and_cert {
        use super::use_key_and_cert;
        use rumqttc::MqttOptions;
        use rumqttc::Transport;
        use tedge_config::TEdgeConfig;
        use tedge_config::TEdgeConfigLocation;

        #[test]
        fn sets_certs_in_the_provided_mqtt_config() {
            let mut opts = MqttOptions::new("dummy-device", "127.0.0.1", 1883);
            let device_cert = rcgen::generate_simple_self_signed(["dummy-device".into()]).unwrap();
            let c8y_cert = rcgen::generate_simple_self_signed(["dummy-c8y".into()]).unwrap();

            let ttd = tedge_test_utils::fs::TempTedgeDir::new();
            let certs_dir = ttd.path().join("device-certs");
            std::fs::create_dir(&certs_dir).unwrap();
            std::fs::write(
                certs_dir.join("tedge-certificate.pem"),
                device_cert.serialize_pem().unwrap(),
            )
            .unwrap();
            std::fs::write(
                certs_dir.join("tedge-private-key.pem"),
                device_cert.serialize_private_key_pem(),
            )
            .unwrap();

            let root_cert_path = ttd.path().join("cloud-certs/c8y.pem");
            std::fs::create_dir(root_cert_path.parent().unwrap()).unwrap();
            std::fs::write(&root_cert_path, c8y_cert.serialize_pem().unwrap()).unwrap();
            let tedge_config =
                TEdgeConfig::try_new(TEdgeConfigLocation::from_custom_root(ttd.path())).unwrap();
            let c8y_config = tedge_config.c8y.try_get::<str>(None).unwrap();

            use_key_and_cert(&mut opts, c8y_config).unwrap();

            let Transport::Tls(tls) = opts.transport() else {
                panic!("Transport should be type TLS")
            };
            let rumqttc::TlsConfiguration::Rustls(config) = tls else {
                panic!("{tls:?} is not Rustls")
            };
            assert!(
                config.client_auth_cert_resolver.has_certs(),
                "Should have certs"
            );
        }
    }

    mod bridge_rule {
        use super::*;

        #[test]
        fn forward_topics_without_any_prefixes() {
            let rule = BridgeRule::try_new("a/topic".into(), "".into(), "".into()).unwrap();
            assert_eq!(rule.apply("a/topic"), Some("a/topic".into()))
        }

        #[test]
        fn forwards_wildcard_topics() {
            let rule = BridgeRule::try_new("a/#".into(), "".into(), "".into()).unwrap();
            assert_eq!(rule.apply("a/topic"), Some("a/topic".into()));
        }

        #[test]
        fn does_not_forward_non_matching_topics() {
            let rule = BridgeRule::try_new("a/topic".into(), "".into(), "".into()).unwrap();
            assert_eq!(rule.apply("different/topic"), None)
        }

        #[test]
        fn removes_local_prefix() {
            let rule = BridgeRule::try_new("topic".into(), "a/".into(), "".into()).unwrap();
            assert_eq!(rule.apply("a/topic"), Some("topic".into()));
        }

        #[test]
        fn prepends_remote_prefix() {
            // TODO maybe debug warn if topic filter begins with prefix to remove
            let rule = BridgeRule::try_new("topic".into(), "a/".into(), "b/".into()).unwrap();
            assert_eq!(rule.apply("a/topic"), Some("b/topic".into()));
        }

        #[test]
        fn does_not_clone_if_topic_is_unchanged() {
            let rule = BridgeRule::try_new("a/topic".into(), "".into(), "".into()).unwrap();
            assert!(matches!(rule.apply("a/topic"), Some(Cow::Borrowed(_))))
        }

        #[test]
        fn does_not_clone_if_prefix_is_removed_but_not_added() {
            let rule = BridgeRule::try_new("topic".into(), "a/".into(), "".into()).unwrap();
            assert!(matches!(rule.apply("a/topic"), Some(Cow::Borrowed(_))))
        }

        #[test]
        fn matches_topics_with_dollar_prefix() {
            let rule =
                BridgeRule::try_new("twin/res/#".into(), "$iothub/".into(), "az/".into()).unwrap();
            assert_eq!(
                rule.apply("$iothub/twin/res/200/?$rid=1"),
                Some("az/twin/res/200/?$rid=1".into())
            )
        }

        #[test]
        fn forwards_unfiltered_topic() {
            let cloud_topic = "thinedge/devices/my-device/test-connection";
            let rule =
                BridgeRule::try_new("".into(), "aws/test-connection".into(), cloud_topic.into())
                    .unwrap();
            assert_eq!(rule.apply("aws/test-connection"), Some(cloud_topic.into()))
        }

        #[test]
        fn allows_empty_input_prefix() {
            let rule = BridgeRule::try_new("test/#".into(), "".into(), "output/".into()).unwrap();
            assert_eq!(rule.apply("test/me"), Some("output/test/me".into()));
        }

        #[test]
        fn allows_empty_output_prefix() {
            let rule = BridgeRule::try_new("test/#".into(), "input/".into(), "".into()).unwrap();
            assert_eq!(rule.apply("input/test/me"), Some("test/me".into()));
        }

        #[test]
        fn rejects_invalid_input_prefix() {
            let err = BridgeRule::try_new("test/#".into(), "wildcard/#".into(), "output/".into())
                .unwrap_err();
            assert_eq!(err.to_string(), "\"wildcard/#\" is not a valid MQTT bridge topic prefix because it contains '+' or '#'");
        }

        #[test]
        fn rejects_invalid_output_prefix() {
            let err = BridgeRule::try_new("test/#".into(), "input/".into(), "wildcard/+".into())
                .unwrap_err();
            assert_eq!(err.to_string(), "\"wildcard/+\" is not a valid MQTT bridge topic prefix because it contains '+' or '#'");
        }

        #[test]
        fn rejects_input_prefix_missing_trailing_slash() {
            let err =
                BridgeRule::try_new("test/#".into(), "input".into(), "output/".into()).unwrap_err();
            assert_eq!(
                err.to_string(),
                "\"input\" is not a valid MQTT bridge topic prefix as it is missing a trailing slash"
            );
        }

        #[test]
        fn rejects_output_prefix_missing_trailing_slash() {
            let err =
                BridgeRule::try_new("test/#".into(), "input/".into(), "output".into()).unwrap_err();
            assert_eq!(
                err.to_string(),
                "\"output\" is not a valid MQTT bridge topic prefix as it is missing a trailing slash"
            );
        }

        #[test]
        fn rejects_empty_input_prefix_with_empty_filter() {
            let err = BridgeRule::try_new("".into(), "".into(), "a/".into()).unwrap_err();
            assert_eq!(
                err.to_string(),
                r#"BridgeRule { topic_filter: "", prefix_to_remove: "", prefix_to_add: "a/" } is not a valid rule, at least one of the topic filter or both prefixes must be non-empty"#
            )
        }

        #[test]
        fn rejects_empty_output_prefix_with_empty_filter() {
            let err = BridgeRule::try_new("".into(), "a/".into(), "".into()).unwrap_err();
            assert_eq!(
                err.to_string(),
                r#"BridgeRule { topic_filter: "", prefix_to_remove: "a/", prefix_to_add: "" } is not a valid rule, at least one of the topic filter or both prefixes must be non-empty"#
            )
        }
    }

    mod topic_converter {
        use super::*;
        #[test]
        fn applies_matching_topic() {
            let converter = TopicConverter(vec![BridgeRule::try_new(
                "topic".into(),
                "a/".into(),
                "b/".into(),
            )
            .unwrap()]);
            assert_eq!(converter.convert_topic("a/topic"), Some("b/topic".into()))
        }

        #[test]
        fn applies_first_matching_topic_if_multiple_are_provided() {
            let converter = TopicConverter(vec![
                BridgeRule::try_new("topic".into(), "a/".into(), "b/".into()).unwrap(),
                BridgeRule::try_new("#".into(), "a/".into(), "c/".into()).unwrap(),
            ]);
            assert_eq!(converter.convert_topic("a/topic"), Some("b/topic".into()));
        }

        #[test]
        fn does_not_apply_non_matching_topics() {
            let converter = TopicConverter(vec![
                BridgeRule::try_new("topic".into(), "x/".into(), "b/".into()).unwrap(),
                BridgeRule::try_new("#".into(), "a/".into(), "c/".into()).unwrap(),
            ]);
            assert_eq!(converter.convert_topic("a/topic"), Some("c/topic".into()));
        }
    }

    mod validate_filter {
        use crate::config::validate_filter;

        #[test]
        fn accepts_wildcard_filters() {
            validate_filter("test/#").unwrap();
        }

        #[test]
        fn accepts_single_topics() {
            validate_filter("valid/topic").unwrap();
        }

        #[test]
        fn includes_supplied_value_in_error_message() {
            let err = validate_filter("invalid/#/filter").unwrap_err();
            assert_eq!(
                err.to_string(),
                "\"invalid/#/filter\" is not a valid MQTT bridge topic filter"
            );
        }
    }
}
