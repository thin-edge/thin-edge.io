use serde::Serialize;
use std::fmt;
use std::str::FromStr;

pub const DEFAULT_SERVICE_TYPE: &str = "service";

/// The name of a service action, e.g, `restart`.
///
/// A name is a single token: `[A-Za-z][A-Za-z0-9_-]*`.
///
/// ```
/// use tedge_api::control::ActionName;
///
/// assert!("restart".parse::<ActionName>().is_ok());
/// assert!("collect_measurements".parse::<ActionName>().is_ok());
/// assert!("is-active".parse::<ActionName>().is_ok());
/// assert!("requestLog".parse::<ActionName>().is_ok());
///
/// assert!("do something".parse::<ActionName>().is_err());
/// assert!("-restart".parse::<ActionName>().is_err());
/// ```
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ActionName(String);

impl FromStr for ActionName {
    type Err = InvalidActionName;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        let invalid = |reason: &str| {
            Err(InvalidActionName {
                name: name.to_string(),
                reason: reason.to_string(),
            })
        };

        if name.is_empty() {
            return invalid("an action name cannot be empty");
        }
        if !name.starts_with(|c: char| c.is_ascii_alphabetic()) {
            return invalid("an action name must start with a letter");
        }
        if !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
        {
            return invalid("an action name must only hold letters, digits, '_' and '-'");
        }

        Ok(ActionName(name.to_string()))
    }
}

impl ActionName {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ActionName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Invalid action name '{name}': {reason}")]
pub struct InvalidActionName {
    pub name: String,
    pub reason: String,
}

/// The name of a service itself, e.g. `collectd`.
///
/// A name is not empty and does not start with `-`, which `systemctl restart --now` and
/// `rc-service --now restart` would read as an option.
///
/// ```
/// use tedge_api::control::ServiceName;
///
/// assert!("collectd".parse::<ServiceName>().is_ok());
/// assert!("getty@tty1".parse::<ServiceName>().is_ok());
/// assert!("dbus-:1.2-org.freedesktop.problems@0".parse::<ServiceName>().is_ok());
///
/// assert!("".parse::<ServiceName>().is_err());
/// assert!("--now".parse::<ServiceName>().is_err());
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceName(String);

impl FromStr for ServiceName {
    type Err = InvalidServiceName;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        let invalid = |reason: &str| {
            Err(InvalidServiceName {
                name: name.to_string(),
                reason: reason.to_string(),
            })
        };

        if name.is_empty() {
            return invalid("a service name cannot be empty");
        }
        if name.starts_with('-') {
            return invalid("a service name cannot start with '-'");
        }

        Ok(ServiceName(name.to_string()))
    }
}

impl ServiceName {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ServiceName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Invalid service name '{name}': {reason}")]
pub struct InvalidServiceName {
    pub name: String,
    pub reason: String,
}

/// The type of a service, e.g., `container`.
///
/// A service type names a file in the service plugin directory, so it must be a plain file name.
/// The non-empty value configured as `service.type` is accepted even when it is not,
/// but it is then never looked up as a plugin file.
///
/// ```
/// use tedge_api::control::ServiceType;
///
/// assert!(ServiceType::new("container", "service").is_ok());
/// assert!(ServiceType::new("Systemd", "Systemd").is_ok());
///
/// assert!(ServiceType::new("../../bin/sh", "service").is_err());
/// assert!(ServiceType::new("Container", "service").is_err());
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceType {
    name: String,
    is_init_system_type: bool,
}

impl ServiceType {
    /// `configured_type` is the `tedge config` value of `service.type`.
    pub fn new(ty: &str, configured_type: &str) -> Result<Self, InvalidServiceType> {
        let is_configured = !ty.is_empty() && ty == configured_type;
        if !is_configured {
            check_plugin_name(ty)?;
        }
        Ok(ServiceType {
            name: ty.to_string(),
            is_init_system_type: is_configured || ty == DEFAULT_SERVICE_TYPE,
        })
    }

    /// Returns `true` for `service` and the configured service type,
    /// which run under the init system when no plugin has their name.
    pub fn is_init_system_type(&self) -> bool {
        self.is_init_system_type
    }

    /// Returns `None` for a configured type that breaks the plugin name rule.
    /// Build a plugin path only from this name.
    pub fn checked_plugin_name(&self) -> Option<&str> {
        Self::is_plugin_name(&self.name).then_some(self.name.as_str())
    }

    pub fn is_plugin_name(ty: &str) -> bool {
        check_plugin_name(ty).is_ok()
    }
}

fn check_plugin_name(ty: &str) -> Result<(), InvalidServiceType> {
    let invalid = |reason: &str| {
        Err(InvalidServiceType {
            ty: ty.to_string(),
            reason: reason.to_string(),
        })
    };

    if ty.is_empty() {
        return invalid("a service type cannot be empty");
    }
    if !ty
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '-'))
    {
        return invalid("a service type must only hold lowercase letters, digits, '_' and '-'");
    }

    Ok(())
}

impl fmt::Display for ServiceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.name.fmt(f)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Invalid service type '{ty}': {reason}")]
pub struct InvalidServiceType {
    pub ty: String,
    pub reason: String,
}

/// The capability a service publishes on its retained `cmd/control` topic when it starts
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlCapability {
    Declaring(&'static [&'static str]),
    Clearing,
}

// Keep this as array so that we can expand the actions later.
pub const TEDGE_SERVICE_ACTIONS: &[&str] = &["restart"];

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[test_case("start")]
    #[test_case("stop")]
    #[test_case("restart")]
    #[test_case("reload")]
    #[test_case("collect_measurements")]
    #[test_case("action_2")]
    #[test_case("ab")]
    #[test_case("is-active"; "with a dash")]
    #[test_case("r"; "single char")]
    #[test_case("RESTART"; "uppercase")]
    #[test_case("requestLog"; "mixed case")]
    #[test_case("Collect_Measurements"; "mixed case with an underscore")]
    fn accepted_action_names(name: &str) {
        assert!(name.parse::<ActionName>().is_ok(), "{name}");
    }

    #[test_case(""; "empty")]
    #[test_case("do something"; "with a space")]
    #[test_case("restart "; "with a trailing space")]
    #[test_case("restart.now"; "with a dot")]
    #[test_case("restart@now"; "with an at sign")]
    #[test_case("_restart"; "starting with an underscore")]
    #[test_case("-restart"; "starting with a dash")]
    #[test_case("2restart"; "starting with a digit")]
    #[test_case("restart/now"; "with a topic separator")]
    #[test_case("restart#"; "with a topic wildcard")]
    #[test_case("restart;reboot"; "with a shell separator")]
    #[test_case("--help"; "looking like an option")]
    fn rejected_action_names(name: &str) {
        assert!(name.parse::<ActionName>().is_err(), "{name}");
    }

    #[test]
    fn the_reason_names_the_rejected_value() {
        let err = "2restart".parse::<ActionName>().unwrap_err();
        assert_eq!(
            err.to_string(),
            "Invalid action name '2restart': an action name must start with a letter"
        );
    }

    #[test_case("collectd")]
    #[test_case("c8y-firmware-plugin")]
    #[test_case("getty@tty1")]
    #[test_case("my.service")]
    #[test_case("Node-RED"; "with uppercase letters")]
    #[test_case("dbus-:1.2-org.freedesktop.problems@0"; "a systemd unit name holding a colon")]
    #[test_case("collectd stop"; "with a space")]
    #[test_case("collectd;reboot"; "with a shell separator")]
    #[test_case("../collectd"; "naming a path")]
    #[test_case("Nginx Web Server"; "a display name")]
    #[test_case("メインサービス"; "not written in ascii")]
    fn accepted_service_names(name: &str) {
        assert!(name.parse::<ServiceName>().is_ok(), "{name}");
    }

    #[test_case(""; "empty")]
    #[test_case("--now"; "looking like an option")]
    #[test_case("-"; "a single dash")]
    fn rejected_service_names(name: &str) {
        assert!(name.parse::<ServiceName>().is_err(), "{name}");
    }

    #[test]
    fn the_reason_names_the_rejected_service_name() {
        let err = "--now".parse::<ServiceName>().unwrap_err();
        assert_eq!(
            err.to_string(),
            "Invalid service name '--now': a service name cannot start with '-'"
        );
    }

    #[test_case("service")]
    #[test_case("container")]
    #[test_case("my_type-2")]
    fn accepted_service_types(ty: &str) {
        let service_type = ServiceType::new(ty, "service").unwrap();
        assert_eq!(service_type.checked_plugin_name(), Some(ty));
    }

    #[test_case(""; "empty")]
    #[test_case("Container"; "with an uppercase letter")]
    #[test_case("../../bin/sh"; "with a path traversal")]
    #[test_case("sub/type"; "with a path separator")]
    #[test_case(".."; "parent directory")]
    fn rejected_service_types(ty: &str) {
        assert!(ServiceType::new(ty, "service").is_err(), "{ty}");
    }

    #[test_case("Systemd"; "with an uppercase letter")]
    #[test_case("Systemd Service"; "with a space")]
    fn the_configured_service_type_is_accepted_but_names_no_plugin(ty: &str) {
        let service_type = ServiceType::new(ty, ty).unwrap();
        assert_eq!(service_type.checked_plugin_name(), None);
    }

    #[test]
    fn an_empty_configured_service_type_is_rejected() {
        assert!(ServiceType::new("", "").is_err());
    }
}
