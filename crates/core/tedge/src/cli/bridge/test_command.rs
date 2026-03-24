use std::borrow::Cow;
use std::io::Write;

use tedge_config::tedge_toml::ProfileName;
use tedge_config::TEdgeConfig;
use tedge_mqtt_bridge::config_toml::Direction;
use tedge_mqtt_bridge::config_toml::ExpandedBridgeRule;
use tedge_mqtt_bridge::BridgeRule;
use yansi::Paint as _;

use super::common::load_bridge_rules;
use super::common::load_bridge_rules_for_custom_mapper;
use super::common::print_non_configurable_or_disabled;
use super::common::print_non_expansions;
use super::common::resolve_cloud;
use super::common::DetailLevel;
use crate::cli::common::Cloud;
use crate::command::Command;
use crate::log::MaybeFancy;

/// Tests where a specific MQTT topic would be forwarded by the bridge
#[derive(clap::Args, Debug, Eq, PartialEq)]
pub struct BridgeTestCmd {
    /// The cloud or custom mapper to test (e.g. c8y, aws, az, or a custom mapper name)
    cloud: String,

    /// The MQTT topic to test (local or remote, wildcards are not supported)
    topic: String,

    /// The cloud profile you wish to use
    ///
    /// [env: TEDGE_CLOUD_PROFILE]
    #[clap(long)]
    profile: Option<ProfileName>,

    /// Show skipped rules (e.g. due to unmet conditions or empty template loops)
    #[clap(long)]
    debug: bool,
}

#[async_trait::async_trait]
impl Command for BridgeTestCmd {
    fn description(&self) -> String {
        format!(
            "test bridge topic routing for {}: {}",
            self.cloud, self.topic
        )
    }

    #[mutants::skip]
    async fn execute(&self, config: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        let detail = if self.debug {
            DetailLevel::Debug
        } else {
            DetailLevel::Normal
        };
        let status = run_test(&mut std::io::stdout(), self, &config, detail).await?;
        match status {
            Status::MatchesFound => std::process::exit(0),
            Status::NoMatches => std::process::exit(2),
        }
    }
}

async fn run_test(
    w: &mut impl Write,
    cmd: &BridgeTestCmd,
    config: &TEdgeConfig,
    detail: DetailLevel,
) -> anyhow::Result<Status> {
    reject_wildcards(&cmd.topic)?;

    match resolve_cloud(&cmd.cloud, cmd.profile.clone()) {
        Some(cloud) => match &cloud {
            #[cfg(feature = "c8y")]
            Cloud::C8y(_) => {
                use tedge_config::tedge_toml::mapper_config::C8yMapperSpecificConfig;
                if let Some((rules, non_expansions)) =
                    load_bridge_rules::<C8yMapperSpecificConfig>(w, config, &cloud, detail).await?
                {
                    if detail == DetailLevel::Debug {
                        print_non_expansions(w, &non_expansions);
                    }
                    if !rules.is_empty() {
                        return Ok(print_topic_matches(w, &cmd.topic, &rules));
                    }
                }
                Ok(Status::NoMatches)
            }
            #[cfg(feature = "aws")]
            Cloud::Aws(_) => {
                print_non_configurable_or_disabled(w, config, &cloud);
                Ok(Status::NoMatches)
            }
            #[cfg(feature = "azure")]
            Cloud::Azure(_) => {
                print_non_configurable_or_disabled(w, config, &cloud);
                Ok(Status::NoMatches)
            }
        },
        None => {
            if let Some((rules, non_expansions)) =
                load_bridge_rules_for_custom_mapper(w, &cmd.cloud, config, detail).await?
            {
                if detail == DetailLevel::Debug {
                    print_non_expansions(w, &non_expansions);
                }
                if !rules.is_empty() {
                    return Ok(print_topic_matches(w, &cmd.topic, &rules));
                }
            }
            Ok(Status::NoMatches)
        }
    }
}

fn reject_wildcards(topic: &str) -> anyhow::Result<()> {
    if topic.contains('#') || topic.contains('+') {
        anyhow::bail!("Wildcard characters (#, +) are not supported. Provide a concrete topic to test against.");
    }
    Ok(())
}

struct TopicMatch {
    local: String,
    remote: String,
    local_rule: String,
    remote_rule: String,
    direction_label: &'static str,
    /// true when the input topic is local (outbound), false when remote (inbound)
    local_to_remote: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Status {
    NoMatches,
    MatchesFound,
}

fn print_topic_matches(w: &mut impl Write, topic: &str, rules: &[ExpandedBridgeRule]) -> Status {
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
                        local_rule: m.rule_input,
                        remote_rule: m.rule_output,
                        direction_label: "(outbound)",
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
                        local_rule: m.rule_output,
                        remote_rule: m.rule_input,
                        direction_label: "(inbound)",
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
                        local_rule: m.rule_input,
                        remote_rule: m.rule_output,
                        direction_label: "(bidirectional)",
                        local_to_remote: true,
                    });
                }
                if let Some(m) =
                    try_match(topic, &rule.remote_prefix, &rule.local_prefix, &rule.topic)
                {
                    matches.push(TopicMatch {
                        local: m.output,
                        remote: m.input,
                        local_rule: m.rule_output,
                        remote_rule: m.rule_input,
                        direction_label: "(bidirectional)",
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
        Status::NoMatches
    } else {
        for m in &matches {
            if m.local_to_remote {
                let _ = writeln!(
                    w,
                    "{} {}  {}  {} {} {}",
                    "[local]".bright_blue(),
                    m.local.bright_blue(),
                    "->".bold(),
                    "[remote]".green(),
                    m.remote.green(),
                    m.direction_label.dim(),
                );
                if m.local_rule != m.local {
                    let _ = writeln!(
                        w,
                        "  {} {} {} {}",
                        "matched by rule:".dim(),
                        m.local_rule.dim(),
                        "->".dim(),
                        m.remote_rule.dim(),
                    );
                }
            } else {
                let _ = writeln!(
                    w,
                    "{} {}  {}  {} {} {}",
                    "[remote]".green(),
                    m.remote.green(),
                    "->".bold(),
                    "[local]".bright_blue(),
                    m.local.bright_blue(),
                    m.direction_label.dim(),
                );
                if m.remote_rule != m.remote {
                    let _ = writeln!(
                        w,
                        "  {} {} {} {}",
                        "matched by rule:".dim(),
                        m.remote_rule.dim(),
                        "->".dim(),
                        m.local_rule.dim(),
                    );
                }
            }
        }
        Status::MatchesFound
    }
}

struct MatchResult {
    input: String,
    output: String,
    rule_input: String,
    rule_output: String,
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
        rule_input: format!("{prefix_to_remove}{base_topic}"),
        rule_output: format!("{prefix_to_add}{base_topic}"),
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

    fn render_test(
        cloud: &str,
        topic: &str,
        profile: Option<ProfileName>,
        config: &TEdgeConfig,
        detail: DetailLevel,
    ) -> String {
        let cmd = BridgeTestCmd {
            cloud: cloud.to_string(),
            topic: topic.to_string(),
            profile,
            debug: detail == DetailLevel::Debug,
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        let mut buf = Vec::new();
        rt.block_on(run_test(&mut buf, &cmd, config, detail))
            .unwrap();
        strip_ansi(&String::from_utf8(buf).unwrap())
    }

    #[test]
    fn c8y_not_connected() {
        let tmp = tempfile::tempdir().unwrap();
        let config = config_with_root(tmp.path(), &c8y_toml(""));
        let output = render_test("c8y", "te/measurements", None, &config, DetailLevel::Normal);
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
        let output = render_test("aws", "some/topic", None, &config, DetailLevel::Normal);
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

        let output = render_test("c8y", "te/measurements", None, &config, DetailLevel::Normal);
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

        let output = render_test("c8y", "c8y/operations", None, &config, DetailLevel::Normal);
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

        let output = render_test("c8y", "te/health", None, &config, DetailLevel::Normal);
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
    fn non_expansions_are_shown_when_a_rule_is_disabled() {
        let tmp = tempfile::tempdir().unwrap();
        let config = config_with_root(tmp.path(), &c8y_toml("c8y.auth_method = 'certificate'"));
        let cloud = Cloud::c8y(None);
        mark_connected(tmp.path(), &cloud);
        write_bridge_toml(
            tmp.path(),
            "test.toml",
            r#"
[[rule]]
if = "${connection.auth_method} == 'password'"
topic = "password-only"
local_prefix = ""
remote_prefix = ""
direction = "outbound"
"#,
        );

        let output = render_test("c8y", "password-only", None, &config, DetailLevel::Debug);
        assert!(
            output.contains("Skipped rules"),
            "Output should mention skipped rules: {output}"
        );
    }

    #[test]
    fn non_expansions_are_not_shown_outside_of_debug_mode() {
        let tmp = tempfile::tempdir().unwrap();
        let config = config_with_root(tmp.path(), &c8y_toml("c8y.auth_method = 'certificate'"));
        let cloud = Cloud::c8y(None);
        mark_connected(tmp.path(), &cloud);
        write_bridge_toml(
            tmp.path(),
            "test.toml",
            r#"
[[rule]]
if = "${connection.auth_method} == 'password'"
topic = "password-only"
local_prefix = ""
remote_prefix = ""
direction = "outbound"
"#,
        );

        let output = render_test("c8y", "password-only", None, &config, DetailLevel::Normal);
        assert!(
            !output.contains("Skipped rules"),
            "Output should not mention skipped rules: {output}"
        );
        assert!(
            output.contains("--debug"),
            "Output should suggest running with '--debug': {output}"
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

        let output = render_test("c8y", "unrelated/topic", None, &config, DetailLevel::Normal);
        assert!(
            output.contains("No matching bridge rule"),
            "output was: {output}"
        );
    }

    fn render_test_err(
        cloud: &str,
        topic: &str,
        profile: Option<ProfileName>,
        config: &TEdgeConfig,
        detail: DetailLevel,
    ) -> String {
        let cmd = BridgeTestCmd {
            cloud: cloud.to_string(),
            topic: topic.to_string(),
            profile,
            debug: detail == DetailLevel::Debug,
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        let mut buf = Vec::new();
        let err = rt
            .block_on(run_test(&mut buf, &cmd, config, detail))
            .unwrap_err();
        format!("{err}")
    }

    #[test]
    fn rejects_hash_wildcard() {
        let tmp = tempfile::tempdir().unwrap();
        let config = config_with_root(tmp.path(), &c8y_toml(""));
        let error = render_test_err("c8y", "c8y/s/us/#", None, &config, DetailLevel::Normal);
        assert!(
            error.contains("Wildcard"),
            "should reject # wildcard: {error}"
        );
    }

    #[test]
    fn rejects_plus_wildcard() {
        let tmp = tempfile::tempdir().unwrap();
        let config = config_with_root(tmp.path(), &c8y_toml(""));
        let error = render_test_err("c8y", "c8y/+/us", None, &config, DetailLevel::Normal);
        assert!(
            error.contains("Wildcard"),
            "should reject + wildcard: {error}"
        );
    }

    #[test]
    fn description_includes_cloud_name_and_topic() {
        let cmd = BridgeTestCmd {
            cloud: "c8y".to_string(),
            topic: "te/measurements".to_string(),
            profile: None,
            debug: false,
        };
        assert_eq!(
            cmd.description(),
            "test bridge topic routing for c8y: te/measurements"
        );
    }

    #[test]
    fn custom_mapper_topic_matches() {
        let tmp = tempfile::tempdir().unwrap();
        let bridge_dir = tmp.path().join("mappers/thingsboard/bridge");
        std::fs::create_dir_all(&bridge_dir).unwrap();
        std::fs::write(
            bridge_dir.join("test.toml"),
            r#"
[[rule]]
local_prefix = "te/"
remote_prefix = "tb/"
direction = "outbound"
topic = "telemetry"
"#,
        )
        .unwrap();
        let config = config_with_root(tmp.path(), "");
        let output = render_test(
            "thingsboard",
            "te/telemetry",
            None,
            &config,
            DetailLevel::Normal,
        );
        assert!(
            output.contains("tb/telemetry"),
            "should show forwarded topic: {output}"
        );
        assert!(
            output.contains("outbound"),
            "should show direction: {output}"
        );
    }

    #[test]
    fn custom_mapper_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let config = config_with_root(tmp.path(), "");
        let output = render_test(
            "thingsboard",
            "some/topic",
            None,
            &config,
            DetailLevel::Normal,
        );
        assert!(
            output.contains("not found"),
            "should indicate mapper not found: {output}"
        );
    }
}
