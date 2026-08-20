//! The service command naming rules shared by the c8y mapper and `tedge service`.

pub const DEFAULT_SERVICE_TYPE: &str = "service";

/// Tells whether a name can be used as a service command action.
pub fn is_valid_action_name(name: &str) -> bool {
    validate_action_name(name).is_ok()
}

/// Check that a name can be used as a service command action.
///
/// A name is a single lowercase token: `[a-z][a-z0-9_-]*`.
///
/// ```
/// use tedge_api::service_command::validate_action_name;
///
/// assert!(validate_action_name("restart").is_ok());
/// assert!(validate_action_name("collect_measurements").is_ok());
/// assert!(validate_action_name("is-active").is_ok());
///
/// assert!(validate_action_name("RESTART").is_err());
/// assert!(validate_action_name("do something").is_err());
/// assert!(validate_action_name("-restart").is_err());
/// ```
pub fn validate_action_name(name: &str) -> Result<(), InvalidActionName> {
    let invalid = |reason: &str| {
        Err(InvalidActionName {
            name: name.to_string(),
            reason: reason.to_string(),
        })
    };

    if name.is_empty() {
        return invalid("an action name cannot be empty");
    }
    if !name.starts_with(|c: char| c.is_ascii_lowercase()) {
        return invalid("an action name must start with a lowercase letter");
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '-'))
    {
        return invalid("an action name must only hold lowercase letters, digits, '_' and '-'");
    }

    Ok(())
}

#[derive(Debug, thiserror::Error)]
#[error("Invalid action name '{name}': {reason}")]
pub struct InvalidActionName {
    pub name: String,
    pub reason: String,
}

/// Check that a name can be used as the name of a service.
///
/// A name is not empty and does not start with `-`, which `systemctl restart --now` and
/// `rc-service --now restart` would read as an option.
///
/// ```
/// use tedge_api::service_command::validate_service_name;
///
/// assert!(validate_service_name("collectd").is_ok());
/// assert!(validate_service_name("getty@tty1").is_ok());
/// assert!(validate_service_name("dbus-:1.2-org.freedesktop.problems@0").is_ok());
///
/// assert!(validate_service_name("").is_err());
/// assert!(validate_service_name("--now").is_err());
/// ```
pub fn validate_service_name(name: &str) -> Result<(), InvalidServiceName> {
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

    Ok(())
}

#[derive(Debug, thiserror::Error)]
#[error("Invalid service name '{name}': {reason}")]
pub struct InvalidServiceName {
    pub name: String,
    pub reason: String,
}

/// Check that a name can be used as the type of a service.
///
/// A service type names a file in the service plugin directory, so it must be a plain file name.
///
/// ```
/// use tedge_api::service_command::validate_service_type;
///
/// assert!(validate_service_type("container").is_ok());
///
/// assert!(validate_service_type("../../bin/sh").is_err());
/// assert!(validate_service_type("Container").is_err());
/// ```
pub fn validate_service_type(ty: &str) -> Result<(), InvalidServiceType> {
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

#[derive(Debug, thiserror::Error)]
#[error("Invalid service type '{ty}': {reason}")]
pub struct InvalidServiceType {
    pub ty: String,
    pub reason: String,
}

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
    fn accepted_action_names(name: &str) {
        assert!(is_valid_action_name(name), "{name} should be accepted");
    }

    #[test_case(""; "empty")]
    #[test_case("RESTART"; "uppercase")]
    #[test_case("myCommand"; "mixed case")]
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
        assert!(!is_valid_action_name(name), "{name} should be rejected");
    }

    #[test]
    fn the_reason_names_the_rejected_value() {
        let err = validate_action_name("Restart").unwrap_err();
        assert_eq!(
            err.to_string(),
            "Invalid action name 'Restart': an action name must start with a lowercase letter"
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
        assert!(validate_service_name(name).is_ok(), "{name}");
    }

    #[test_case(""; "empty")]
    #[test_case("--now"; "looking like an option")]
    #[test_case("-"; "a single dash")]
    fn rejected_service_names(name: &str) {
        assert!(validate_service_name(name).is_err(), "{name}");
    }

    #[test]
    fn the_reason_names_the_rejected_service_name() {
        let err = validate_service_name("--now").unwrap_err();
        assert_eq!(
            err.to_string(),
            "Invalid service name '--now': a service name cannot start with '-'"
        );
    }

    #[test_case("service")]
    #[test_case("container")]
    #[test_case("my_type-2")]
    fn accepted_service_types(ty: &str) {
        assert!(validate_service_type(ty).is_ok(), "{ty}");
    }

    #[test_case(""; "empty")]
    #[test_case("Container"; "with an uppercase letter")]
    #[test_case("../../bin/sh"; "with a path traversal")]
    #[test_case("sub/type"; "with a path separator")]
    #[test_case(".."; "parent directory")]
    fn rejected_service_types(ty: &str) {
        assert!(validate_service_type(ty).is_err(), "{ty}");
    }
}
