use std::io::Write;

use pad::PadStr;
use tedge_mqtt_bridge::config_toml::Direction;
use tedge_mqtt_bridge::config_toml::ExpandedBridgeRule;
use yansi::Paint as _;

use super::common::cloud_name;
use super::common::load_bridge_rules;
use super::common::print_non_configurable_or_disabled;
use super::common::print_non_expansions;
use super::common::DetailLevel;
use crate::cli::common::Cloud;
use crate::cli::common::CloudArg;
use crate::command::Command;
use crate::log::MaybeFancy;
use tedge_config::TEdgeConfig;

/// Shows the current bridge configuration
#[derive(clap::Args, Debug, Eq, PartialEq)]
pub struct BridgeInspectCmd {
    #[clap(subcommand)]
    cloud: CloudArg,

    #[clap(long, global = true)]
    debug: bool,
}

#[async_trait::async_trait]
impl Command for BridgeInspectCmd {
    fn description(&self) -> String {
        let cloud_name = cloud_name(&Cloud::try_from(self.cloud.clone()).unwrap());
        format!("inspect the bridge configuration for {cloud_name}")
    }

    #[mutants::skip]
    async fn execute(&self, config: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        let detail = if self.debug {
            DetailLevel::Debug
        } else {
            DetailLevel::Normal
        };
        run_inspect(&mut std::io::stdout(), &self.cloud, &config, detail).await?;
        Ok(())
    }
}

async fn run_inspect(
    w: &mut impl Write,
    cloud_arg: &CloudArg,
    config: &TEdgeConfig,
    detail: DetailLevel,
) -> anyhow::Result<()> {
    let cloud = Cloud::try_from(cloud_arg.clone())?;

    match cloud_arg {
        #[cfg(feature = "c8y")]
        CloudArg::C8y { .. } => {
            use tedge_config::tedge_toml::mapper_config::C8yMapperSpecificConfig;
            if let Some((rules, non_expansions)) =
                load_bridge_rules::<C8yMapperSpecificConfig>(w, config, &cloud, detail).await?
            {
                if detail == DetailLevel::Debug {
                    print_non_expansions(w, &non_expansions);
                }
                if !rules.is_empty() {
                    print_rules(w, rules);
                }
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

fn print_rules(w: &mut impl Write, rules: Vec<ExpandedBridgeRule>) {
    let (bidir, dir): (Vec<_>, Vec<_>) = rules
        .into_iter()
        .partition(|rule| rule.direction == Direction::Bidirectional);
    let (outbound, inbound): (Vec<_>, Vec<_>) = dir
        .into_iter()
        .partition(|rule| rule.direction == Direction::Outbound);

    print_outbound_rules(w, &outbound);
    print_inbound_rules(w, &inbound);
    print_bidirectional_rules(w, &bidir);
}

fn print_outbound_rules(w: &mut impl Write, rules: &[ExpandedBridgeRule]) {
    let max_width = rules
        .iter()
        .map(|r| r.local_prefix.len() + r.topic.len())
        .max()
        .unwrap_or(0);

    let _ = writeln!(
        w,
        "{} {} {}",
        "Local".bold().bright_blue(),
        "->".bold(),
        "Remote".bold().green()
    );

    if rules.is_empty() {
        let _ = writeln!(w, "  {}", "-- No matching rules --".dim());
    } else {
        for rule in rules {
            let local = format!("{}{}", rule.local_prefix, rule.topic);
            let remote = format!("{}{}", rule.remote_prefix, rule.topic);
            let _ = writeln!(
                w,
                "  {}  {}  {}",
                local
                    .pad_to_width_with_alignment(max_width, pad::Alignment::Left)
                    .bright_blue(),
                "->".bold(),
                remote.green()
            );
        }
    }
    let _ = writeln!(w);
}

fn print_inbound_rules(w: &mut impl Write, rules: &[ExpandedBridgeRule]) {
    let max_width = rules
        .iter()
        .map(|r| r.remote_prefix.len() + r.topic.len())
        .max()
        .unwrap_or(0);

    let _ = writeln!(
        w,
        "{} {} {}",
        "Remote".bold().green(),
        "->".bold(),
        "Local".bold().bright_blue()
    );

    if rules.is_empty() {
        let _ = writeln!(w, "  {}", "-- No matching rules --".dim());
    } else {
        for rule in rules {
            let remote = format!("{}{}", rule.remote_prefix, rule.topic);
            let local = format!("{}{}", rule.local_prefix, rule.topic);
            let _ = writeln!(
                w,
                "  {}  {}  {}",
                remote
                    .pad_to_width_with_alignment(max_width, pad::Alignment::Left)
                    .green(),
                "->".bold(),
                local.bright_blue()
            );
        }
    }
    let _ = writeln!(w);
}

fn print_bidirectional_rules(w: &mut impl Write, rules: &[ExpandedBridgeRule]) {
    let max_width = rules
        .iter()
        .map(|r| r.local_prefix.len() + r.topic.len())
        .max()
        .unwrap_or(0);

    let _ = writeln!(w, "{}", "Bidirectional".bold().yellow());

    if rules.is_empty() {
        let _ = writeln!(w, "  {}", "-- No matching rules --".dim());
    } else {
        for rule in rules {
            let local = format!("{}{}", rule.local_prefix, rule.topic);
            let remote = format!("{}{}", rule.remote_prefix, rule.topic);
            let _ = writeln!(
                w,
                "  {}  {}  {}",
                local
                    .pad_to_width_with_alignment(max_width, pad::Alignment::Left)
                    .bright_blue(),
                "<->".bold().yellow(),
                remote.green()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::cli::bridge::common::render;
    use crate::cli::bridge::common::strip_ansi;

    use super::*;

    #[test]
    fn outbound_rules_shows_local_to_remote() {
        let output = render(|w| {
            print_outbound_rules(
                w,
                &[rule(Direction::Outbound, "te/", "c8y/", "measurements")],
            );
        });

        assert!(output.contains("Local"), "should have Local header");
        assert!(output.contains("Remote"), "should have Remote header");
        assert!(
            output.contains("te/measurements"),
            "should show local topic"
        );
        assert!(
            output.contains("c8y/measurements"),
            "should show remote topic"
        );
        assert!(output.contains("->"), "should show arrow");
    }

    #[test]
    fn inbound_rules_shows_remote_to_local() {
        let output = render(|w| {
            print_inbound_rules(w, &[rule(Direction::Inbound, "te/", "c8y/", "operations")]);
        });

        assert!(output.contains("c8y/operations"));
        assert!(output.contains("te/operations"));
    }

    #[test]
    fn bidirectional_rules_shows_both_directions() {
        let output = render(|w| {
            print_bidirectional_rules(
                w,
                &[rule(Direction::Bidirectional, "te/", "c8y/", "health")],
            );
        });

        assert!(output.contains("Bidirectional"));
        assert!(output.contains("te/health"));
        let line = output.lines().find(|l| l.contains("te/health")).unwrap();
        assert_eq!(depad_line(line), "te/health <-> c8y/health");
    }

    #[test]
    fn empty_rules_shows_no_matching() {
        let output = render(|w| print_outbound_rules(w, &[]));
        assert!(output.contains("No matching rules"));
    }

    #[test]
    fn print_rules_partitions_by_direction() {
        let rules = vec![
            rule(Direction::Outbound, "te/", "c8y/", "measurements"),
            rule(Direction::Inbound, "c8y/", "te/", "operations"),
            rule(Direction::Bidirectional, "te/", "c8y/", "health"),
        ];

        let output = render(|w| print_rules(w, rules));

        // All three sections should be present
        assert!(output.contains("Local"));
        assert!(output.contains("Remote"));
        assert!(output.contains("Bidirectional"));
        // All topics should appear
        assert!(output.contains("measurements"));
        assert!(output.contains("operations"));
        assert!(output.contains("health"));
    }

    #[test]
    fn outbound_rules_aligns_columns() {
        let rules = vec![
            rule(Direction::Outbound, "te/", "c8y/", "short"),
            rule(
                Direction::Outbound,
                "te/device/main/",
                "c8y/s/",
                "longer-topic",
            ),
        ];

        let output = render(|w| print_outbound_rules(w, &rules));

        pretty_assertions::assert_eq!(depad_multiline(&output), "Local -> Remote\nte/short -> c8y/short\nte/device/main/longer-topic -> c8y/s/longer-topic\n")
    }

    #[test]
    fn c8y_not_connected() {
        let tmp = tempfile::tempdir().unwrap();
        let config = config_with_root(tmp.path(), &c8y_toml(""));
        let output = render_inspect(
            &CloudArg::C8y { profile: None },
            &config,
            DetailLevel::Normal,
        );
        assert!(
            output.contains("Not connected to Cumulocity"),
            "output was: {output}"
        );
    }

    #[test]
    fn c8y_no_bridge_config_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let config = config_with_root(tmp.path(), &c8y_toml(""));
        let cloud = Cloud::c8y(None);
        mark_connected(tmp.path(), &cloud);

        let output = render_inspect(
            &CloudArg::C8y { profile: None },
            &config,
            DetailLevel::Normal,
        );
        assert!(
            output.contains("No bridge configuration directory found"),
            "output was: {output}"
        );
    }

    #[test]
    fn c8y_empty_bridge_config_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let config = config_with_root(tmp.path(), &c8y_toml(""));
        let cloud = Cloud::c8y(None);
        mark_connected(tmp.path(), &cloud);
        std::fs::create_dir_all(tmp.path().join("mappers/c8y/bridge")).unwrap();

        let output = render_inspect(
            &CloudArg::C8y { profile: None },
            &config,
            DetailLevel::Normal,
        );
        assert!(
            output.contains("No bridge configuration files found"),
            "output was: {output}"
        );
    }

    #[test]
    fn c8y_uses_active_auth_method() {
        let tmp = tempfile::tempdir().unwrap();
        let config = config_with_root(tmp.path(), &c8y_toml("c8y.auth_method = 'basic'"));
        let cloud = Cloud::c8y(None);
        mark_connected(tmp.path(), &cloud);
        std::fs::create_dir_all(tmp.path().join("mappers/c8y/bridge")).unwrap();
        std::fs::write(
            tmp.path().join("mappers/c8y/bridge/test.toml"),
            r#"
        [[rule]]
        if = "${connection.auth_method} == 'password'"
        topic = "password-only"
        local_prefix = ""
        remote_prefix = ""
        direction = "outbound"
        "#,
        )
        .unwrap();

        let output = render_inspect(
            &CloudArg::C8y { profile: None },
            &config,
            DetailLevel::Normal,
        );
        assert!(
            !output.contains("Skipped rules"),
            "Output should not mention skipped rules as none were skipped: {output}"
        );
        let start = output.find("Local -> Remote").unwrap();
        let output = depad_multiline(&output[start..]);
        pretty_assertions::assert_eq!(
            output,
            "
Local -> Remote
password-only -> password-only

Remote -> Local
-- No matching rules --

Bidirectional
-- No matching rules --
"
            .trim()
        );
    }

    #[test]
    fn invalid_bridge_toml_prevents_further_output() {
        let tmp = tempfile::tempdir().unwrap();
        let config = config_with_root(tmp.path(), &c8y_toml(""));
        let cloud = Cloud::c8y(None);
        mark_connected(tmp.path(), &cloud);
        std::fs::create_dir_all(tmp.path().join("mappers/c8y/bridge")).unwrap();
        // Not a valid toml file, doesn't have any required keys inside rule
        std::fs::write(
            tmp.path().join("mappers/c8y/bridge/test.toml"),
            r#"
        [[rule]]
        "#,
        )
        .unwrap();

        let output = render_inspect(
            &CloudArg::C8y { profile: None },
            &config,
            DetailLevel::Normal,
        );
        assert!(
            output.contains("Failed to read bridge config files"),
            "should show error message, actual output: {output}"
        );
        assert!(
            !output.contains("Local -> Remote"),
            "should not show any rules, actual output: {output}"
        );
    }

    #[test]
    fn c8y_built_in_bridge_disabled_not_connected() {
        let tmp = tempfile::tempdir().unwrap();
        let config = config_with_root(
            tmp.path(),
            "c8y.url = \"example.cumulocity.com\"\n\
             mqtt.bridge.built_in = false\n",
        );
        let output = render_inspect(
            &CloudArg::C8y { profile: None },
            &config,
            DetailLevel::Normal,
        );
        assert!(
            output.contains("Built-in bridge is disabled"),
            "output was: {output}"
        );
        assert!(
            output.contains("Not connected"),
            "should show not connected when no mosquitto config: {output}"
        );
    }

    #[test]
    fn c8y_built_in_bridge_disabled_with_mosquitto_config() {
        let tmp = tempfile::tempdir().unwrap();
        let config = config_with_root(
            tmp.path(),
            "c8y.url = \"example.cumulocity.com\"\n\
             mqtt.bridge.built_in = false\n",
        );
        let cloud = Cloud::c8y(None);
        mark_connected(tmp.path(), &cloud);

        let output = render_inspect(
            &CloudArg::C8y { profile: None },
            &config,
            DetailLevel::Normal,
        );
        assert!(
            output.contains("Built-in bridge is disabled"),
            "output was: {output}"
        );
        assert!(
            output.contains("mosquitto bridge config is stored in"),
            "should show mosquitto config path: {output}"
        );
    }

    #[test]
    fn aws_built_in_bridge_not_configurable() {
        let tmp = tempfile::tempdir().unwrap();
        let config = config_with_root(
            tmp.path(),
            "aws.url = \"example.amazonaws.com\"\n\
             mqtt.bridge.built_in = true\n",
        );
        let output = render_inspect(
            &CloudArg::Aws { profile: None },
            &config,
            DetailLevel::Normal,
        );
        assert!(
            output.contains("not yet configurable"),
            "output was: {output}"
        );
        assert!(output.contains("AWS"), "output was: {output}");
    }

    #[test]
    fn aws_built_in_bridge_disabled() {
        let tmp = tempfile::tempdir().unwrap();
        let config = config_with_root(
            tmp.path(),
            "aws.url = \"example.amazonaws.com\"\n\
             mqtt.bridge.built_in = false\n",
        );
        let output = render_inspect(
            &CloudArg::Aws { profile: None },
            &config,
            DetailLevel::Normal,
        );
        assert!(
            output.contains("Built-in bridge is disabled"),
            "output was: {output}"
        );
    }

    #[test]
    fn azure_built_in_bridge_not_configurable() {
        let tmp = tempfile::tempdir().unwrap();
        let config = config_with_root(
            tmp.path(),
            "az.url = \"example.azure-devices.net\"\n\
             mqtt.bridge.built_in = true\n",
        );
        let output = render_inspect(
            &CloudArg::Az { profile: None },
            &config,
            DetailLevel::Normal,
        );
        assert!(
            output.contains("not yet configurable"),
            "output was: {output}"
        );
        assert!(output.contains("Azure"), "output was: {output}");
    }

    #[test]
    fn azure_built_in_bridge_disabled() {
        let tmp = tempfile::tempdir().unwrap();
        let config = config_with_root(
            tmp.path(),
            "az.url = \"example.azure-devices.net\"\n\
             mqtt.bridge.built_in = false\n",
        );
        let output = render_inspect(
            &CloudArg::Az { profile: None },
            &config,
            DetailLevel::Normal,
        );
        assert!(
            output.contains("Built-in bridge is disabled"),
            "output was: {output}"
        );
    }

    #[test]
    fn non_expansions_are_shown_when_a_rule_is_disabled() {
        let tmp = tempfile::tempdir().unwrap();
        let config = config_with_root(tmp.path(), &c8y_toml("c8y.auth_method = 'certificate'"));
        let cloud = Cloud::c8y(None);
        mark_connected(tmp.path(), &cloud);
        std::fs::create_dir_all(tmp.path().join("mappers/c8y/bridge")).unwrap();
        std::fs::write(
            tmp.path().join("mappers/c8y/bridge/test.toml"),
            r#"
        [[rule]]
        if = "${connection.auth_method} == 'password'"
        topic = "password-only"
        local_prefix = ""
        remote_prefix = ""
        direction = "outbound"
        "#,
        )
        .unwrap();

        let output = render_inspect(
            &CloudArg::C8y { profile: None },
            &config,
            DetailLevel::Debug,
        );
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
        std::fs::create_dir_all(tmp.path().join("mappers/c8y/bridge")).unwrap();
        std::fs::write(
            tmp.path().join("mappers/c8y/bridge/test.toml"),
            r#"
        [[rule]]
        if = "${connection.auth_method} == 'password'"
        topic = "password-only"
        local_prefix = ""
        remote_prefix = ""
        direction = "outbound"
        "#,
        )
        .unwrap();

        let output = render_inspect(
            &CloudArg::C8y { profile: None },
            &config,
            DetailLevel::Normal,
        );
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
    fn disabled_files_are_reported() {
        let tmp = tempfile::tempdir().unwrap();
        let config = config_with_root(tmp.path(), &c8y_toml(""));
        let cloud = Cloud::c8y(None);
        mark_connected(tmp.path(), &cloud);
        let bridge_dir = tmp.path().join("mappers/c8y/bridge");
        std::fs::create_dir_all(&bridge_dir).unwrap();
        std::fs::write(
            bridge_dir.join("test.toml"),
            r#"
[[rule]]
topic = "measurements"
local_prefix = "te/"
remote_prefix = "c8y/"
direction = "outbound"
"#,
        )
        .unwrap();
        // Create the disabled marker
        std::fs::write(bridge_dir.join("test.toml.disabled"), "").unwrap();

        let output = render_inspect(
            &CloudArg::C8y { profile: None },
            &config,
            DetailLevel::Normal,
        );
        assert!(
            output.contains("Skipping:"),
            "should mention skipped file: {output}"
        );
        assert!(
            output.contains("test.toml"),
            "should mention the disabled filename: {output}"
        );
        assert!(
            output.contains("disabled"),
            "should mention disabled: {output}"
        );
    }

    #[test]
    fn outbound_rules_are_correctly_padded() {
        let rules = vec![
            rule(Direction::Outbound, "te/", "c8y/", "short"),
            rule(
                Direction::Outbound,
                "te/device/main/",
                "c8y/s/",
                "longer-topic",
            ),
        ];

        let output = render(|w| print_outbound_rules(w, &rules));

        pretty_assertions::assert_eq!(
            output,
            "\
Local -> Remote
  te/short                     ->  c8y/short
  te/device/main/longer-topic  ->  c8y/s/longer-topic
\n"
        );
    }

    #[test]
    fn inbound_rules_are_correctly_padded() {
        let rules = vec![
            rule(Direction::Inbound, "te/", "c8y/", "short"),
            rule(
                Direction::Inbound,
                "te/device/main/",
                "c8y/s/",
                "longer-topic",
            ),
        ];

        let output = render(|w| print_inbound_rules(w, &rules));

        pretty_assertions::assert_eq!(
            output,
            "\
Remote -> Local
  c8y/short           ->  te/short
  c8y/s/longer-topic  ->  te/device/main/longer-topic
\n"
        );
    }

    #[test]
    fn bidirectional_rules_are_correctly_padded() {
        let rules = vec![
            rule(Direction::Bidirectional, "te/", "c8y/", "short"),
            rule(
                Direction::Bidirectional,
                "te/device/main/",
                "c8y/s/",
                "longer-topic",
            ),
        ];

        let output = render(|w| print_bidirectional_rules(w, &rules));

        pretty_assertions::assert_eq!(
            output,
            "\
Bidirectional
  te/short                     <->  c8y/short
  te/device/main/longer-topic  <->  c8y/s/longer-topic
"
        );
    }

    #[test]
    fn description_includes_cloud_name() {
        let cmd = BridgeInspectCmd {
            cloud: CloudArg::C8y { profile: None },
            debug: false,
        };
        assert_eq!(
            cmd.description(),
            "inspect the bridge configuration for Cumulocity"
        );
    }

    fn rule(
        direction: Direction,
        local_prefix: &str,
        remote_prefix: &str,
        topic: &str,
    ) -> ExpandedBridgeRule {
        ExpandedBridgeRule {
            direction,
            local_prefix: local_prefix.into(),
            remote_prefix: remote_prefix.into(),
            topic: topic.into(),
        }
    }

    fn depad_multiline(text: &str) -> String {
        text.lines().map(depad_line).collect::<Vec<_>>().join("\n")
    }

    fn depad_line(line: &str) -> String {
        line.trim()
            .chars()
            .scan(false, |was_space, c| {
                if c == ' ' {
                    if *was_space {
                        Some(None)
                    } else {
                        *was_space = true;
                        Some(Some(' '))
                    }
                } else {
                    *was_space = false;
                    Some(Some(c))
                }
            })
            .flatten()
            .collect()
    }

    #[track_caller]
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

    /// Create the mosquitto config file that signals the cloud is connected
    fn mark_connected(root: &std::path::Path, cloud: &Cloud) {
        let dir = root.join("mosquitto-conf");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(&*cloud.mosquitto_config_filename()), "").unwrap();
    }

    fn render_inspect(cloud_arg: &CloudArg, config: &TEdgeConfig, detail: DetailLevel) -> String {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let mut buf = Vec::new();
        rt.block_on(run_inspect(&mut buf, cloud_arg, config, detail))
            .unwrap();
        strip_ansi(&String::from_utf8(buf).unwrap())
    }
}
