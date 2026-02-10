use std::io::Write;

use camino::Utf8Path;
use camino::Utf8PathBuf;
use tedge_config::models::CloudType;
use tedge_config::tedge_toml::mapper_config::ExpectedCloudType;
use tedge_config::tedge_toml::ProfileName;
use tedge_config::TEdgeConfig;
use tedge_mqtt_bridge::config_toml::ExpandedBridgeRule;
use tedge_mqtt_bridge::config_toml::NonExpansionReason;
use tedge_mqtt_bridge::visit_bridge_config_dir;
use tedge_mqtt_bridge::AuthMethod;
use tedge_mqtt_bridge::BridgeConfigVisitor;
use yansi::Paint as _;

use crate::cli::common::MaybeBorrowedCloud;

pub fn cloud_name(cloud: &MaybeBorrowedCloud<'_>) -> &'static str {
    match cloud {
        #[cfg(feature = "c8y")]
        MaybeBorrowedCloud::C8y { .. } => "Cumulocity",
        #[cfg(feature = "aws")]
        MaybeBorrowedCloud::Aws { .. } => "AWS",
        #[cfg(feature = "azure")]
        MaybeBorrowedCloud::Azure { .. } => "Azure",
    }
}

/// Context needed to display a non-expansion reason with source location
pub struct NonExpansionContext {
    pub path: Utf8PathBuf,
    pub source: String,
    pub reason: NonExpansionReason,
}

/// Print messages for clouds that don't yet support configurable bridge rules.
///
/// If the built-in bridge is disabled, shows the mosquitto config path.
/// Otherwise, shows that the cloud's bridge rules are not yet configurable.
pub fn print_non_configurable_or_disabled(
    w: &mut impl Write,
    config: &TEdgeConfig,
    cloud: &MaybeBorrowedCloud<'_>,
) {
    if !config.mqtt.bridge.built_in {
        print_built_in_bridge_disabled(w, config, cloud);
    } else {
        print_built_in_bridge_non_configurable(w, cloud);
    }
}

/// Loads bridge rules after performing all precondition checks.
///
/// Returns `None` if an early-return message was already printed to `w`
/// (e.g. bridge disabled, not connected, no config directory).
pub async fn load_bridge_rules<Cloud: ExpectedCloudType>(
    w: &mut impl Write,
    config: &TEdgeConfig,
    profile: &Option<ProfileName>,
    cloud: &MaybeBorrowedCloud<'_>,
) -> anyhow::Result<Option<(Vec<ExpandedBridgeRule>, Vec<NonExpansionContext>)>> {
    let bridge_config_dir = config
        .mapper_config_dir::<Cloud>(profile.as_ref())
        .join("bridge");

    if !config.mqtt.bridge.built_in {
        print_built_in_bridge_disabled(w, config, cloud);
        return Ok(None);
    }

    let mosquitto_config_path = config
        .root_dir()
        .join("mosquitto-conf")
        .join(&*cloud.mosquitto_config_filename());
    if !mosquitto_config_path.exists() {
        print_not_connected(w, cloud);
        return Ok(None);
    }

    print_header(w, profile, &bridge_config_dir, cloud);

    if !bridge_config_dir.exists() {
        writeln!(w, "{}", "No bridge configuration directory found.".yellow())?;
        writeln!(
            w,
            "The bridge configuration will be created when the mapper starts."
        )?;
        return Ok(None);
    }

    let auth_method = get_auth_method::<Cloud>(config, profile)?;
    let mut visitor = InspectVisitor::new();

    if let Err(e) = visit_bridge_config_dir(
        &bridge_config_dir,
        config,
        auth_method,
        profile.as_ref(),
        &mut visitor,
    )
    .await
    {
        tracing::error!("{e:#}");
        writeln!(w, "{}", "Failed to read bridge config files".red())?;
        return Ok(None);
    }

    match visitor.status {
        Status::NoTemplates => {
            writeln!(w, "{}", "No bridge configuration files found.".yellow())?;
            return Ok(None);
        }
        Status::Empty => {
            writeln!(
                w,
                "{}",
                "Bridge config files exist, but no rules were generated".yellow()
            )?;
            writeln!(
                w,
                "{} {}",
                "Help:".blue().bold(),
                "Try running with the `--debug` flag to see more information on disabled rules"
                    .blue()
            )?;
            return Ok(None);
        }
        Status::NonEmpty => (),
    }

    Ok(Some((visitor.rules, visitor.non_expansions)))
}

pub fn print_built_in_bridge_disabled(
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

pub fn print_not_connected(w: &mut impl Write, cloud: &MaybeBorrowedCloud<'_>) {
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

fn get_auth_method<Cloud: ExpectedCloudType>(
    config: &TEdgeConfig,
    profile: &Option<ProfileName>,
) -> anyhow::Result<AuthMethod> {
    match Cloud::expected_cloud_type() {
        CloudType::C8y => Ok(tedge_mapper::c8y::mapper::auth_method(
            &config.c8y_mapper_config(profile)?,
        )),
        _ => Ok(AuthMethod::Certificate),
    }
}

struct InspectVisitor {
    rules: Vec<ExpandedBridgeRule>,
    non_expansions: Vec<NonExpansionContext>,
    status: Status,
}

impl InspectVisitor {
    fn new() -> Self {
        Self {
            rules: Vec::new(),
            non_expansions: Vec::new(),
            status: Status::default(),
        }
    }
}

impl BridgeConfigVisitor for InspectVisitor {
    fn on_file_disabled(&mut self, path: &Utf8Path) {
        let filename = path.file_name().unwrap_or("unknown");
        println!("{} {} (disabled)", "Skipping:".dim(), filename.dim());
    }

    fn on_rules_loaded(
        &mut self,
        path: &Utf8Path,
        source: &str,
        rules: Vec<ExpandedBridgeRule>,
        non_expansions: Vec<NonExpansionReason>,
    ) -> anyhow::Result<()> {
        self.status += if rules.is_empty() {
            Status::Empty
        } else {
            Status::NonEmpty
        };
        self.rules.extend(rules);
        for reason in non_expansions {
            self.non_expansions.push(NonExpansionContext {
                path: path.to_owned(),
                source: source.to_owned(),
                reason,
            });
        }
        Ok(())
    }
}

#[must_use]
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(strum::EnumIter))]
enum Status {
    #[default]
    NoTemplates,
    Empty,
    NonEmpty,
}

impl std::ops::Add for Status {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            // If either is the default status -> straight to the other status
            (Status::NoTemplates, other) | (other, Status::NoTemplates) => other,
            // If we've not read any rules -> doesn't affect other status
            (Status::Empty, rhs) => rhs,
            // If we have read rules, that remains true
            (Status::NonEmpty, _) => Status::NonEmpty,
        }
    }
}

impl std::ops::AddAssign for Status {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

#[cfg(test)]
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
pub fn strip_ansi(s: &str) -> String {
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

#[cfg(test)]
mod tests {
    use strum::IntoEnumIterator as _;

    use super::*;

    macro_rules! assert_status_add {
        ($lhs:expr, $rhs:expr => $expected:expr) => {
            let (lhs, rhs, expected) = ($lhs, $rhs, $expected);
            let actual = lhs + rhs;
            assert_eq!(
                actual, expected,
                "{lhs:?} + {rhs:?} = {actual:?}, expected {expected:?}"
            );
        };
    }

    #[test]
    fn non_empty_always_propagates() {
        for status in Status::iter() {
            assert_status_add!(Status::NonEmpty, status => Status::NonEmpty);
            assert_status_add!(status, Status::NonEmpty => Status::NonEmpty);
        }
    }

    #[test]
    fn default_status_does_not_propagate() {
        for status in Status::iter() {
            assert_status_add!(Status::NoTemplates, status => status);
            assert_status_add!(status, Status::NoTemplates => status);
        }
    }

    #[test]
    fn emptiness_does_not_propagate() {
        for status in [Status::Empty, Status::NonEmpty] {
            assert_status_add!(Status::Empty, status => status);
            assert_status_add!(status, Status::Empty => status);
        }
    }
}
