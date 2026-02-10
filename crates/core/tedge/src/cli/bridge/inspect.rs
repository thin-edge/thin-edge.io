use std::io::Write;

use anyhow::Context;
use ariadne::Color;
use ariadne::Config;
use ariadne::Label;
use ariadne::Report;
use ariadne::ReportKind;
use ariadne::Source;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use pad::PadStr;
use tedge_config::models::CloudType;
use tedge_config::tedge_toml::mapper_config::ExpectedCloudType;
use tedge_config::tedge_toml::ProfileName;
use tedge_config::TEdgeConfig;
use tedge_mqtt_bridge::config_toml::Direction;
use tedge_mqtt_bridge::config_toml::ExpandedBridgeRule;
use tedge_mqtt_bridge::config_toml::NonExpansionReason;
use tedge_mqtt_bridge::expand_bridge_rules;
use tedge_mqtt_bridge::AuthMethod;
use yansi::Paint as _;

use crate::cli::common::Cloud;
use crate::cli::common::CloudArg;
use crate::cli::common::MaybeBorrowedCloud;
use crate::command::Command;
use crate::log::MaybeFancy;

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

    async fn execute(&self, config: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        run_inspect(&mut std::io::stdout(), &self.cloud, &config, self.debug).await?;
        Ok(())
    }
}

async fn run_inspect(
    w: &mut impl Write,
    cloud_arg: &CloudArg,
    config: &TEdgeConfig,
    debug: bool,
) -> anyhow::Result<()> {
    let cloud = Cloud::try_from(cloud_arg.clone())?;

    match cloud_arg {
        #[cfg(feature = "c8y")]
        CloudArg::C8y { profile } => {
            use tedge_config::tedge_toml::mapper_config::C8yMapperSpecificConfig;
            inspect_bridge::<C8yMapperSpecificConfig>(w, config, profile, &cloud, debug).await?;
        }
        #[cfg(feature = "aws")]
        CloudArg::Aws { .. } => {
            if !config.mqtt.bridge.built_in {
                print_built_in_bridge_disabled(w, config, &cloud);
            } else {
                print_built_in_bridge_non_configurable(w, &cloud);
            }
        }
        #[cfg(feature = "azure")]
        CloudArg::Az { .. } => {
            if !config.mqtt.bridge.built_in {
                print_built_in_bridge_disabled(w, config, &cloud);
            } else {
                print_built_in_bridge_non_configurable(w, &cloud);
            }
        }
    }

    Ok(())
}

async fn inspect_bridge<Cloud: ExpectedCloudType>(
    w: &mut impl Write,
    config: &TEdgeConfig,
    profile: &Option<ProfileName>,
    cloud: &MaybeBorrowedCloud<'_>,
    debug: bool,
) -> anyhow::Result<()> {
    let bridge_config_dir = config
        .mapper_config_dir::<Cloud>(profile.as_ref())
        .join("bridge");

    if !config.mqtt.bridge.built_in {
        print_built_in_bridge_disabled(w, config, cloud);
        return Ok(());
    }

    let mosquitto_config_path = config
        .root_dir()
        .join("mosquitto-conf")
        .join(&*cloud.mosquitto_config_filename());
    if !mosquitto_config_path.exists() {
        print_not_connected(w, cloud);
        return Ok(());
    }

    print_header(w, profile, &bridge_config_dir, cloud);

    if !bridge_config_dir.exists() {
        writeln!(w, "{}", "No bridge configuration directory found.".yellow())?;
        writeln!(
            w,
            "The bridge configuration will be created when the mapper starts."
        )?;
        return Ok(());
    }

    let auth_method = get_auth_method::<Cloud>(config, profile)?;
    let (rules, non_expansions) =
        load_rules_from_directory(&bridge_config_dir, config, auth_method, profile).await?;

    if rules.is_empty() && non_expansions.is_empty() {
        writeln!(w, "{}", "No bridge configuration files found.".yellow())?;
        return Ok(());
    }

    if debug {
        print_non_expansions(w, &non_expansions);
    }

    print_rules(w, rules);

    Ok(())
}

fn print_built_in_bridge_disabled(
    w: &mut impl Write,
    config: &TEdgeConfig,
    cloud: &MaybeBorrowedCloud<'_>,
) {
    let _ = writeln!(w, "{}", "Built-in bridge is disabled".yellow());
    let mosquitto_config_path = config
        .root_dir()
        .join("mosquitto-conf")
        .join(&*cloud.mosquitto_config_filename());
    if mosquitto_config_path.exists() {
        let _ = writeln!(
            w,
            "The mosquitto bridge config is stored in {}",
            mosquitto_config_path.bright_blue()
        );
    } else {
        print_not_connected(w, cloud);
    }
}

fn print_not_connected(w: &mut impl Write, cloud: &MaybeBorrowedCloud<'_>) {
    let name = cloud_name(cloud).blue();
    if let Some(profile) = cloud.profile_name() {
        let _ = writeln!(
            w,
            "Not connected to {name} with profile {}",
            profile.green()
        );
    } else {
        let _ = writeln!(w, "Not connected to {name}");
    }
}

fn print_built_in_bridge_non_configurable(w: &mut impl Write, cloud: &MaybeBorrowedCloud<'_>) {
    let _ = writeln!(
        w,
        "Built-in bridge rules are not yet configurable for {}",
        cloud_name(cloud).yellow()
    );
}

fn print_header(
    w: &mut impl Write,
    profile: &Option<ProfileName>,
    bridge_config_dir: &camino::Utf8PathBuf,
    cloud: &MaybeBorrowedCloud<'_>,
) {
    let _ = writeln!(w, "{} {}", "Bridge configuration for".bold(), cloud.bold());
    if let Some(profile) = profile {
        let _ = writeln!(w, "Profile: {}", profile.green());
    }
    let _ = writeln!(w, "Reading from: {}", bridge_config_dir.bright_blue());
    let _ = writeln!(w);
}

// TODO can this not be duplicated?
fn get_auth_method<Cloud: ExpectedCloudType>(
    config: &TEdgeConfig,
    profile: &Option<ProfileName>,
) -> anyhow::Result<AuthMethod> {
    match Cloud::expected_cloud_type() {
        CloudType::C8y => {
            let c8y_config = config.c8y_mapper_config(profile)?;
            let use_certificate = c8y_config
                .cloud_specific
                .auth_method
                .is_certificate(&c8y_config.cloud_specific.credentials_path);

            Ok(if use_certificate {
                AuthMethod::Certificate
            } else {
                AuthMethod::Password
            })
        }
        _ => Ok(AuthMethod::Certificate),
    }
}

/// Context needed to display a non-expansion reason with source location
struct NonExpansionContext {
    path: Utf8PathBuf,
    source: String,
    reason: NonExpansionReason,
}

async fn load_rules_from_directory(
    bridge_config_dir: &camino::Utf8PathBuf,
    config: &TEdgeConfig,
    auth_method: AuthMethod,
    profile: &Option<ProfileName>,
) -> anyhow::Result<(Vec<ExpandedBridgeRule>, Vec<NonExpansionContext>)> {
    let mut all_rules = Vec::new();
    let mut all_non_expansions = Vec::new();

    let mut read_dir = tokio::fs::read_dir(bridge_config_dir)
        .await
        .with_context(|| format!("Failed to read bridge config directory: {bridge_config_dir}"))?;

    while let Some(entry) = read_dir.next_entry().await? {
        let path = entry.path();
        let Some(utf8_path) = path.to_str().map(Utf8Path::new) else {
            continue;
        };

        if utf8_path.extension() != Some("toml") {
            continue;
        }

        if is_disabled(utf8_path).await {
            let filename = utf8_path.file_name().unwrap_or("unknown");
            println!("{} {} (disabled)", "Skipping:".dim(), filename.dim());
            continue;
        }

        process_config_file(
            utf8_path,
            config,
            auth_method,
            profile,
            &mut all_rules,
            &mut all_non_expansions,
        )
        .await;
    }

    Ok((all_rules, all_non_expansions))
}

async fn is_disabled(path: &Utf8Path) -> bool {
    let disabled_path = path.with_extension("toml.disabled");
    tokio::fs::try_exists(&disabled_path).await.unwrap_or(false)
}

async fn process_config_file(
    utf8_path: &Utf8Path,
    config: &TEdgeConfig,
    auth_method: AuthMethod,
    profile: &Option<ProfileName>,
    all_rules: &mut Vec<ExpandedBridgeRule>,
    all_non_expansions: &mut Vec<NonExpansionContext>,
) {
    let toml_content = match tokio::fs::read_to_string(utf8_path).await {
        Ok(content) => content,
        Err(e) => {
            let filename = utf8_path.file_name().unwrap_or("unknown");
            eprintln!(
                "{} Failed to read {}: {}",
                "Error:".red().bold(),
                filename,
                e
            );
            return;
        }
    };

    match expand_bridge_rules(
        utf8_path,
        &toml_content,
        config,
        auth_method,
        profile.as_ref(),
    ) {
        Ok((rules, non_expansions)) => {
            all_rules.extend(rules);
            for reason in non_expansions {
                all_non_expansions.push(NonExpansionContext {
                    path: utf8_path.to_owned(),
                    source: toml_content.clone(),
                    reason,
                });
            }
        }
        Err(e) => {
            let filename = utf8_path.file_name().unwrap_or("unknown");
            eprintln!(
                "{} Error parsing {}: {}",
                "Error:".red().bold(),
                filename,
                e
            );
        }
    }
}

fn print_non_expansions(w: &mut impl Write, non_expansions: &[NonExpansionContext]) {
    if non_expansions.is_empty() {
        return;
    }

    let _ = writeln!(w, "{}", "Skipped rules:".blue().bold());
    let _ = writeln!(w);

    for ctx in non_expansions {
        let (main_message, cause_span, message, rule_span) = match &ctx.reason {
            NonExpansionReason::ConditionIsFalse {
                span,
                message,
                rule_span,
            } => (
                "Rule skipped",
                span.clone(),
                message.as_str(),
                rule_span.clone(),
            ),
            NonExpansionReason::LoopSourceEmpty {
                src,
                message,
                rule_span,
            } => (
                "Template rule generated no rules",
                src.span(),
                message.as_str(),
                Some(rule_span.clone()),
            ),
        };

        let path = &ctx.path;
        let mut report = Report::build(ReportKind::Advice, (path.as_str(), cause_span.clone()))
            .with_config(Config::default().with_compact(false))
            .with_message(main_message);

        // Add label for the entire rule context if available
        if let Some(rule_span) = rule_span {
            report = report.with_label(
                Label::new((path.as_str(), rule_span))
                    .with_message("this rule was skipped")
                    .with_color(Color::Blue),
            );
        }

        // Add label for the specific cause
        report = report.with_label(
            Label::new((path.as_str(), cause_span))
                .with_message(message)
                .with_color(Color::Yellow),
        );

        report
            .finish()
            .write((path.as_str(), Source::from(&ctx.source)), &mut *w)
            .unwrap();
    }
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

fn cloud_name(cloud: &MaybeBorrowedCloud<'_>) -> &'static str {
    match cloud {
        #[cfg(feature = "c8y")]
        MaybeBorrowedCloud::C8y { .. } => "Cumulocity",
        #[cfg(feature = "aws")]
        MaybeBorrowedCloud::Aws { .. } => "AWS",
        #[cfg(feature = "azure")]
        MaybeBorrowedCloud::Azure { .. } => "Azure",
    }
}

#[cfg(test)]
mod tests {
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
        assert_eq!(depad(line), "te/health <-> c8y/health");
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

    fn render_inspect(cloud_arg: &CloudArg, config: &TEdgeConfig, debug: bool) -> String {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let mut buf = Vec::new();
        rt.block_on(run_inspect(&mut buf, cloud_arg, config, debug))
            .unwrap();
        strip_ansi(&String::from_utf8(buf).unwrap())
    }

    #[test]
    fn c8y_not_connected() {
        let tmp = tempfile::tempdir().unwrap();
        let config = config_with_root(tmp.path(), &c8y_toml(""));
        let output = render_inspect(&CloudArg::C8y { profile: None }, &config, false);
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

        let output = render_inspect(&CloudArg::C8y { profile: None }, &config, false);
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

        let output = render_inspect(&CloudArg::C8y { profile: None }, &config, false);
        assert!(
            output.contains("No bridge configuration files found"),
            "output was: {output}"
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
        let output = render_inspect(&CloudArg::C8y { profile: None }, &config, false);
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

        let output = render_inspect(&CloudArg::C8y { profile: None }, &config, false);
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
        let output = render_inspect(&CloudArg::Aws { profile: None }, &config, false);
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
        let output = render_inspect(&CloudArg::Aws { profile: None }, &config, false);
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
        let output = render_inspect(&CloudArg::Az { profile: None }, &config, false);
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
        let output = render_inspect(&CloudArg::Az { profile: None }, &config, false);
        assert!(
            output.contains("Built-in bridge is disabled"),
            "output was: {output}"
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

    /// Render to a string with yansi colors disabled
    fn render(f: impl FnOnce(&mut Vec<u8>)) -> String {
        let mut buf = Vec::new();
        f(&mut buf);
        strip_ansi(&String::from_utf8(buf).unwrap())
    }

    /// A very rudimentary solution for stripping the ansi escape sequences
    ///
    /// The code uses yansi to color code bits of output. The tests don't need
    /// to care about this formatting, so we can remove it.
    ///
    /// Warning: This is not a particularly robust way to solve the problem. It
    /// is some simple code generated by gemini. A better solution would be the
    /// `strip-ansi-escapes` crate, but this should be fine for the needs of
    /// these tests.
    ///
    /// A possible other solution is calling `yansi::disable()` before rendering
    /// output, but that means we lose the formatting on [pretty_assertions]. If
    /// we re-enable after rendering, that breaks when running under `cargo
    /// test` as that runs the tests in a single process in separate threads.
    fn strip_ansi(s: &str) -> String {
        let mut result = String::new();
        let mut inside_ansi = false;

        for c in s.chars() {
            if c == '\x1b' {
                inside_ansi = true;
            } else if inside_ansi && c == 'm' {
                inside_ansi = false;
            } else if !inside_ansi {
                result.push(c);
            }
        }
        result
    }

    fn depad_multiline(text: &str) -> String {
        text.lines().map(depad).collect::<Vec<_>>().join("\n")
    }

    fn depad(line: &str) -> String {
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
}
