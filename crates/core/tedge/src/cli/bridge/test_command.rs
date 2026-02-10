use std::borrow::Cow;
use std::io::Write;

use tedge_config::tedge_toml::ProfileName;
use tedge_config::TEdgeConfig;
use tedge_mqtt_bridge::config_toml::Direction;
use tedge_mqtt_bridge::config_toml::ExpandedBridgeRule;
use tedge_mqtt_bridge::BridgeRule;
use yansi::Paint as _;

use super::common::cloud_name;
use super::common::load_bridge_rules;
use super::common::print_non_configurable_or_disabled;
use crate::cli::common::Cloud;
use crate::cli::common::CloudArg;
use crate::command::Command;
use crate::log::MaybeFancy;

/// Tests where a specific MQTT topic would be forwarded by the bridge
#[derive(clap::Args, Debug, Eq, PartialEq)]
pub struct BridgeTestCmd {
    #[clap(subcommand)]
    cloud: CloudTopicArg,

    #[clap(long, global = true)]
    debug: bool,
}

#[derive(clap::Subcommand, Debug, Clone, Eq, PartialEq)]
enum CloudTopicArg {
    #[cfg(feature = "c8y")]
    C8y {
        /// The cloud profile you wish to use
        ///
        /// [env: TEDGE_CLOUD_PROFILE]
        #[clap(long)]
        profile: Option<ProfileName>,

        /// The MQTT topic to test
        topic: String,
    },
    #[cfg(feature = "aws")]
    Aws {
        /// The cloud profile you wish to use
        ///
        /// [env: TEDGE_CLOUD_PROFILE]
        #[clap(long)]
        profile: Option<ProfileName>,

        /// The MQTT topic to test
        topic: String,
    },
    #[cfg(feature = "azure")]
    Az {
        /// The cloud profile you wish to use
        ///
        /// [env: TEDGE_CLOUD_PROFILE]
        #[clap(long)]
        profile: Option<ProfileName>,

        /// The MQTT topic to test
        topic: String,
    },
}

impl CloudTopicArg {
    fn cloud_arg(&self) -> CloudArg {
        match self {
            #[cfg(feature = "c8y")]
            Self::C8y { profile, .. } => CloudArg::C8y {
                profile: profile.clone(),
            },
            #[cfg(feature = "aws")]
            Self::Aws { profile, .. } => CloudArg::Aws {
                profile: profile.clone(),
            },
            #[cfg(feature = "azure")]
            Self::Az { profile, .. } => CloudArg::Az {
                profile: profile.clone(),
            },
        }
    }

    fn topic(&self) -> &str {
        match self {
            #[cfg(feature = "c8y")]
            Self::C8y { topic, .. } => topic,
            #[cfg(feature = "aws")]
            Self::Aws { topic, .. } => topic,
            #[cfg(feature = "azure")]
            Self::Az { topic, .. } => topic,
        }
    }
}

#[async_trait::async_trait]
impl Command for BridgeTestCmd {
    fn description(&self) -> String {
        let cloud_arg = self.cloud.cloud_arg();
        let cloud_name = cloud_name(&Cloud::try_from(cloud_arg).unwrap());
        format!(
            "test bridge topic routing for {cloud_name}: {}",
            self.cloud.topic()
        )
    }

    async fn execute(&self, config: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        run_test(&mut std::io::stdout(), &self.cloud, &config, self.debug).await?;
        Ok(())
    }
}

async fn run_test(
    w: &mut impl Write,
    cloud_topic: &CloudTopicArg,
    config: &TEdgeConfig,
    _debug: bool,
) -> anyhow::Result<()> {
    let cloud_arg = cloud_topic.cloud_arg();
    let cloud = Cloud::try_from(cloud_arg.clone())?;
    let topic = cloud_topic.topic();

    match &cloud_arg {
        #[cfg(feature = "c8y")]
        CloudArg::C8y { profile } => {
            use tedge_config::tedge_toml::mapper_config::C8yMapperSpecificConfig;
            if let Some((rules, _non_expansions)) =
                load_bridge_rules::<C8yMapperSpecificConfig>(w, config, profile, &cloud).await?
            {
                print_topic_matches(w, topic, &rules);
            }
        }
        #[cfg(feature = "aws")]
        CloudArg::Aws { .. } => {
            print_non_configurable_or_disabled(w, config, &cloud);
        }
        #[cfg(feature = "azure")]
        CloudArg::Az { .. } => {
            print_non_configurable_or_disabled(w, config, &cloud);
        }
    }

    Ok(())
}

struct TopicMatch {
    local: String,
    remote: String,
    direction_label: &'static str,
    /// true when the input topic is local (outbound), false when remote (inbound)
    local_to_remote: bool,
}

fn print_topic_matches(w: &mut impl Write, topic: &str, rules: &[ExpandedBridgeRule]) {
    let mut matches = Vec::new();

    for rule in rules {
        match rule.direction {
            Direction::Outbound => {
                if let Some(m) =
                    try_match(topic, &rule.local_prefix, &rule.remote_prefix, &rule.topic)
                {
                    matches.push(TopicMatch {
                        local: m.input,
                        remote: m.output,
                        direction_label: "outbound (local -> remote)",
                        local_to_remote: true,
                    });
                }
            }
            Direction::Inbound => {
                if let Some(m) =
                    try_match(topic, &rule.remote_prefix, &rule.local_prefix, &rule.topic)
                {
                    matches.push(TopicMatch {
                        local: m.output,
                        remote: m.input,
                        direction_label: "inbound (remote -> local)",
                        local_to_remote: false,
                    });
                }
            }
            Direction::Bidirectional => {
                if let Some(m) =
                    try_match(topic, &rule.local_prefix, &rule.remote_prefix, &rule.topic)
                {
                    matches.push(TopicMatch {
                        local: m.input,
                        remote: m.output,
                        direction_label: "bidirectional (local -> remote)",
                        local_to_remote: true,
                    });
                }
                if let Some(m) =
                    try_match(topic, &rule.remote_prefix, &rule.local_prefix, &rule.topic)
                {
                    matches.push(TopicMatch {
                        local: m.output,
                        remote: m.input,
                        direction_label: "bidirectional (remote -> local)",
                        local_to_remote: false,
                    });
                }
            }
        }
    }

    if matches.is_empty() {
        let _ = writeln!(
            w,
            "{}",
            format!("No matching bridge rule found for \"{topic}\"").yellow()
        );
    } else {
        for m in &matches {
            if m.local_to_remote {
                let _ = writeln!(
                    w,
                    "{}  {}  {}  ({})",
                    m.local.bright_blue(),
                    "->".bold(),
                    m.remote.green(),
                    m.direction_label.dim()
                );
            } else {
                let _ = writeln!(
                    w,
                    "{}  {}  {}  ({})",
                    m.remote.green(),
                    "->".bold(),
                    m.local.bright_blue(),
                    m.direction_label.dim()
                );
            }
        }
    }
}

struct MatchResult {
    input: String,
    output: String,
}

fn try_match(
    topic: &str,
    prefix_to_remove: &str,
    prefix_to_add: &str,
    base_topic: &str,
) -> Option<MatchResult> {
    let bridge_rule = BridgeRule::try_new(
        Cow::Owned(base_topic.to_owned()),
        Cow::Owned(prefix_to_remove.to_owned()),
        Cow::Owned(prefix_to_add.to_owned()),
    )
    .ok()?;

    let output = bridge_rule.apply(topic)?;
    Some(MatchResult {
        input: topic.to_owned(),
        output: output.into_owned(),
    })
}

#[cfg(test)]
mod tests {
    use crate::cli::bridge::common::strip_ansi;

    use super::*;

    fn config_with_root(root: &std::path::Path, toml: &str) -> TEdgeConfig {
        TEdgeConfig::load_toml_str_with_root_dir(root, toml)
    }

    fn c8y_toml(extra: &str) -> String {
        format!(
            "c8y.url = \"example.cumulocity.com\"\n\
             mqtt.bridge.built_in = true\n\
             {extra}"
        )
    }

    fn mark_connected(root: &std::path::Path, cloud: &Cloud) {
        let dir = root.join("mosquitto-conf");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(&*cloud.mosquitto_config_filename()), "").unwrap();
    }

    fn write_bridge_toml(root: &std::path::Path, filename: &str, content: &str) {
        let bridge_dir = root.join("mappers/c8y/bridge");
        std::fs::create_dir_all(&bridge_dir).unwrap();
        std::fs::write(bridge_dir.join(filename), content).unwrap();
    }

    fn render_test(cloud_topic: &CloudTopicArg, config: &TEdgeConfig, debug: bool) -> String {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let mut buf = Vec::new();
        rt.block_on(run_test(&mut buf, cloud_topic, config, debug))
            .unwrap();
        strip_ansi(&String::from_utf8(buf).unwrap())
    }

    #[test]
    fn c8y_not_connected() {
        let tmp = tempfile::tempdir().unwrap();
        let config = config_with_root(tmp.path(), &c8y_toml(""));
        let output = render_test(
            &CloudTopicArg::C8y {
                profile: None,
                topic: "te/measurements".into(),
            },
            &config,
            false,
        );
        assert!(
            output.contains("Not connected to Cumulocity"),
            "output was: {output}"
        );
    }

    #[test]
    fn aws_not_configurable() {
        let tmp = tempfile::tempdir().unwrap();
        let config = config_with_root(
            tmp.path(),
            "aws.url = \"example.amazonaws.com\"\n\
             mqtt.bridge.built_in = true\n",
        );
        let output = render_test(
            &CloudTopicArg::Aws {
                profile: None,
                topic: "some/topic".into(),
            },
            &config,
            false,
        );
        assert!(
            output.contains("not yet configurable"),
            "output was: {output}"
        );
    }

    #[test]
    fn topic_matches_outbound_rule() {
        let tmp = tempfile::tempdir().unwrap();
        let config = config_with_root(tmp.path(), &c8y_toml(""));
        let cloud = Cloud::c8y(None);
        mark_connected(tmp.path(), &cloud);
        write_bridge_toml(
            tmp.path(),
            "test.toml",
            r#"
[[rule]]
local_prefix = "te/"
remote_prefix = "c8y/"
direction = "outbound"
topic = "measurements"
"#,
        );

        let output = render_test(
            &CloudTopicArg::C8y {
                profile: None,
                topic: "te/measurements".into(),
            },
            &config,
            false,
        );
        assert!(
            output.contains("c8y/measurements"),
            "should show forwarded topic: {output}"
        );
        assert!(
            output.contains("outbound"),
            "should show direction: {output}"
        );
    }

    #[test]
    fn topic_matches_inbound_rule() {
        let tmp = tempfile::tempdir().unwrap();
        let config = config_with_root(tmp.path(), &c8y_toml(""));
        let cloud = Cloud::c8y(None);
        mark_connected(tmp.path(), &cloud);
        write_bridge_toml(
            tmp.path(),
            "test.toml",
            r#"
[[rule]]
local_prefix = "te/"
remote_prefix = "c8y/"
direction = "inbound"
topic = "operations"
"#,
        );

        let output = render_test(
            &CloudTopicArg::C8y {
                profile: None,
                topic: "c8y/operations".into(),
            },
            &config,
            false,
        );
        assert!(
            output.contains("te/operations"),
            "should show forwarded topic: {output}"
        );
        assert!(
            output.contains("inbound"),
            "should show direction: {output}"
        );
    }

    #[test]
    fn topic_matches_bidirectional_rule() {
        let tmp = tempfile::tempdir().unwrap();
        let config = config_with_root(tmp.path(), &c8y_toml(""));
        let cloud = Cloud::c8y(None);
        mark_connected(tmp.path(), &cloud);
        write_bridge_toml(
            tmp.path(),
            "test.toml",
            r#"
[[rule]]
local_prefix = "te/"
remote_prefix = "c8y/"
direction = "bidirectional"
topic = "health"
"#,
        );

        let output = render_test(
            &CloudTopicArg::C8y {
                profile: None,
                topic: "te/health".into(),
            },
            &config,
            false,
        );
        assert!(
            output.contains("c8y/health"),
            "should show forwarded topic: {output}"
        );
        assert!(
            output.contains("bidirectional"),
            "should show direction: {output}"
        );
    }

    #[test]
    fn topic_matches_no_rules() {
        let tmp = tempfile::tempdir().unwrap();
        let config = config_with_root(tmp.path(), &c8y_toml(""));
        let cloud = Cloud::c8y(None);
        mark_connected(tmp.path(), &cloud);
        write_bridge_toml(
            tmp.path(),
            "test.toml",
            r#"
[[rule]]
local_prefix = "te/"
remote_prefix = "c8y/"
direction = "outbound"
topic = "measurements"
"#,
        );

        let output = render_test(
            &CloudTopicArg::C8y {
                profile: None,
                topic: "unrelated/topic".into(),
            },
            &config,
            false,
        );
        assert!(
            output.contains("No matching bridge rule"),
            "output was: {output}"
        );
    }
}
