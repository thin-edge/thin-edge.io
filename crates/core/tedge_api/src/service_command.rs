//! The naming rule shared by everything that handles a service command action.
//!
//! An action is named by the `cmd/<action>` topic segment of a service entity,
//! and the same name is given to `tedge service <action> <service-name>`.
//! Both the c8y mapper, which turns a cloud command name into a topic,
//! and the CLI, which passes the name to an execution backend, validate it with this rule.

/// The longest accepted action name.
const MAX_ACTION_NAME_LEN: usize = 64;

/// Tells whether a name can be used as a service command action.
///
/// A name is a single lowercase token: `[a-z][a-z0-9_]+`, of bounded length.
/// MQTT topic names are case-sensitive and do accept spaces, so this is a restriction
/// thin-edge puts on itself, to keep an action name unchanged along the way from a cloud
/// command name to a topic segment and then to the argument of a command line.
///
/// ```
/// use tedge_api::service_command::is_valid_action_name;
///
/// assert!(is_valid_action_name("restart"));
/// assert!(is_valid_action_name("collect_measurements"));
///
/// assert!(!is_valid_action_name("RESTART"));
/// assert!(!is_valid_action_name("do something"));
/// assert!(!is_valid_action_name("restart-now"));
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
    if name.len() < 2 {
        return invalid("an action name is at least 2 characters long");
    }
    if !name.starts_with(|c: char| c.is_ascii_lowercase()) {
        return invalid("an action name starts with a lowercase letter");
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        return invalid("an action name only holds lowercase letters, digits and '_'");
    }

    Ok(())
}

#[derive(Debug, thiserror::Error)]
#[error("Invalid action name '{name}': {reason}")]
pub struct InvalidActionName {
    pub name: String,
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
    fn accepted(name: &str) {
        assert!(is_valid_action_name(name), "{name} should be accepted");
    }

    #[test_case(""; "empty")]
    #[test_case("r"; "single char")]
    #[test_case("RESTART"; "uppercase")]
    #[test_case("myCommand"; "mixed case")]
    #[test_case("do something"; "with a space")]
    #[test_case("restart "; "with a trailing space")]
    #[test_case("restart-now"; "with a dash")]
    #[test_case("_restart"; "starting with an underscore")]
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
}
