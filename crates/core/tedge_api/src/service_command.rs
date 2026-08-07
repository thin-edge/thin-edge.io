//! The naming rules shared by everything that handles a service command.
//!
//! An action is named by the `cmd/<action>` topic segment of a service entity,
//! and the same name is given to `tedge service <action> <service-name>`.
//! Both the c8y mapper, which turns a cloud command name into a topic,
//! and the CLI, which passes the name to an execution backend, validate it with this rule.
//!
//! A service name is checked by the same two, for the same reason:
//! the mapper takes it from the cloud operation and the CLI passes it to a backend.
//!
//! The action name rule is the largest set of characters that survives every step a name takes:
//!
//! - the c8y mapper lowercases the command name it receives from the cloud, so a name must be
//!   unchanged by lowercasing: no uppercase letter;
//! - the name is a segment of the `cmd/<action>` topic: no `/`, `+`, `#` and no space;
//! - the name is passed as one argument to an init tool or to a service plugin: no leading `-`;
//! - the name is a key of `[init]` in `system.toml`, so it must be a TOML bare key, which accepts
//!   only letters, digits, `_` and `-`. This is what leaves `.` and `@` out, even though a service
//!   name accepts both.

/// The longest accepted action name.
const MAX_ACTION_NAME_LEN: usize = 64;

/// The longest accepted service name.
const MAX_SERVICE_NAME_LEN: usize = 128;

/// The longest accepted service type.
const MAX_SERVICE_TYPE_LEN: usize = 64;

/// The type given to a service which is managed by the init system.
///
/// This is the type `tedge service` assumes when none is given, and the type the c8y mapper
/// puts in a command when the target service was registered without a type.
pub const DEFAULT_SERVICE_TYPE: &str = "service";

/// Tells whether a name can be used as a service command action.
///
/// A name is a single lowercase token: `[a-z][a-z0-9_-]*`, of bounded length.
/// MQTT topic names are case-sensitive and do accept spaces, so this is a restriction
/// thin-edge puts on itself, to keep an action name unchanged along the way from a cloud
/// command name to a topic segment and then to the argument of a command line.
/// See the module documentation for what each character of the rule comes from.
///
/// ```
/// use tedge_api::service_command::is_valid_action_name;
///
/// assert!(is_valid_action_name("restart"));
/// assert!(is_valid_action_name("collect_measurements"));
/// assert!(is_valid_action_name("is-active"));
///
/// assert!(!is_valid_action_name("RESTART"));
/// assert!(!is_valid_action_name("do something"));
/// assert!(!is_valid_action_name("-restart"));
/// ```
pub fn is_valid_action_name(name: &str) -> bool {
    validate_action_name(name).is_ok()
}

/// Check that a name can be used as a service command action.
///
/// Return the reason why not, ready to be reported to whoever provided the name.
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
    if name.len() > MAX_ACTION_NAME_LEN {
        return invalid(&format!(
            "an action name cannot be longer than {MAX_ACTION_NAME_LEN} characters"
        ));
    }
    if !name.starts_with(|c: char| c.is_ascii_lowercase()) {
        return invalid("an action name starts with a lowercase letter");
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '-'))
    {
        return invalid("an action name only holds lowercase letters, digits, '_' and '-'");
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
/// A service name comes from the cloud and is passed as one argument to an init tool or to a
/// service plugin, so it must not be read as an option: nothing stops a name from being `--now`.
///
/// thin-edge itself never builds a shell command line, but it cannot promise that of the backends
/// it calls. An `[init]` template is an argv list the user writes, so it can be
/// `["/bin/sh", "-c", "systemctl restart {}"]`, and a service plugin may be a shell script that
/// uses its arguments unquoted. So the rule below is a whitelist of the characters a real service
/// name needs, not a list of the characters known to be dangerous.
///
/// Return the reason why not, ready to be reported to whoever provided the name.
///
/// Unlike an action name, a service name is not lowercased and does accept `.`, `@` and `-`:
/// it names a unit, a container or whatever the backend manages, and thin-edge does not choose
/// how those are named. Being a whitelist, the rule is still narrower than those backends:
/// a systemd unit name may hold `:`, and this rule does not accept it.
///
/// ```
/// use tedge_api::service_command::validate_service_name;
///
/// assert!(validate_service_name("collectd").is_ok());
/// assert!(validate_service_name("getty@tty1").is_ok());
///
/// assert!(validate_service_name("--now").is_err());
/// assert!(validate_service_name("collectd;reboot").is_err());
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
    if name.len() > MAX_SERVICE_NAME_LEN {
        return invalid(&format!(
            "a service name cannot be longer than {MAX_SERVICE_NAME_LEN} characters"
        ));
    }
    if name.starts_with('-') {
        return invalid("a service name cannot start with '-'");
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '@' | '-'))
    {
        return invalid("a service name only holds letters, digits, '_', '.', '@' and '-'");
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
/// A service type selects a file in the service plugin directory, so it must resolve to a plain
/// file name and must not allow path traversal. Return the reason why not, ready to be reported
/// to whoever provided the type.
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
    if ty.len() > MAX_SERVICE_TYPE_LEN {
        return invalid(&format!(
            "a service type cannot be longer than {MAX_SERVICE_TYPE_LEN} characters"
        ));
    }
    if !ty
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '-'))
    {
        return invalid("a service type only holds lowercase letters, digits, '_' and '-'");
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
    fn accepted(name: &str) {
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
    fn rejected(name: &str) {
        assert!(!is_valid_action_name(name), "{name} should be rejected");
    }

    #[test]
    fn a_too_long_name_is_rejected() {
        let name = "a".repeat(MAX_ACTION_NAME_LEN);
        assert!(is_valid_action_name(&name));

        let name = "a".repeat(MAX_ACTION_NAME_LEN + 1);
        assert!(!is_valid_action_name(&name));
    }

    #[test]
    fn the_reason_names_the_rejected_value() {
        let err = validate_action_name("Restart").unwrap_err();
        assert_eq!(
            err.to_string(),
            "Invalid action name 'Restart': an action name starts with a lowercase letter"
        );
    }

    #[test_case("collectd")]
    #[test_case("c8y-firmware-plugin")]
    #[test_case("getty@tty1")]
    #[test_case("my.service")]
    #[test_case("Node-RED"; "with uppercase letters")]
    fn accepted_service_names(name: &str) {
        assert!(validate_service_name(name).is_ok(), "{name}");
    }

    #[test_case(""; "empty")]
    #[test_case("--now"; "looking like an option")]
    #[test_case("collectd stop"; "with a space")]
    #[test_case("collectd;reboot"; "with a shell separator")]
    #[test_case("../collectd"; "with a path")]
    fn rejected_service_names(name: &str) {
        assert!(validate_service_name(name).is_err(), "{name}");
    }

    #[test]
    fn a_too_long_service_name_is_rejected() {
        let name = "a".repeat(MAX_SERVICE_NAME_LEN);
        assert!(validate_service_name(&name).is_ok());

        let name = "a".repeat(MAX_SERVICE_NAME_LEN + 1);
        assert!(validate_service_name(&name).is_err());
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

    #[test]
    fn a_too_long_service_type_is_rejected() {
        let ty = "a".repeat(MAX_SERVICE_TYPE_LEN);
        assert!(validate_service_type(&ty).is_ok());

        let ty = "a".repeat(MAX_SERVICE_TYPE_LEN + 1);
        assert!(validate_service_type(&ty).is_err());
    }
}
