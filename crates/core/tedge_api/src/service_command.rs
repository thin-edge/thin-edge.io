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
//!   only letters, digits, `_` and `-`. This is what leaves `.` and `@` out, characters a service
//!   name is free to hold.

/// The type given to a service which is managed by the init system.
///
/// This is the type `tedge service` assumes when none is given, and the type the c8y mapper
/// puts in a command when the target service was registered without a type.
pub const DEFAULT_SERVICE_TYPE: &str = "service";

/// Tells whether a name can be used as a service command action.
///
/// A name is a single lowercase token: `[a-z][a-z0-9_-]*`.
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
/// A name is refused only where a step it takes says so: it is not empty, and it does not start
/// with `-`, since `systemctl restart --now` and `rc-service --now restart` read that argument as
/// an option and the name stops being a name.
///
/// No character is refused: a service name is whatever the device registered the service under,
/// and a backend gets it as a single argument.
///
/// Return the reason why not, ready to be reported to whoever provided the name.
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

/// The name under which the agent registers its own service entity.
const TEDGE_AGENT: &str = "tedge-agent";

/// The prefix of every mapper name, whatever its cloud, topic prefix and profile.
const MAPPER_PREFIX: &str = "tedge-mapper-";

/// The two mappers which are connected to no cloud.
const CLOUDLESS_MAPPERS: [&str; 2] = ["tedge-mapper-collectd", "tedge-mapper-local"];

/// The five actions an init system defines in `system.toml`.
const ALL_ACTIONS: &[&str] = &["start", "stop", "restart", "enable", "disable"];

/// The actions of a service thin-edge needs in order to answer whoever asked.
///
/// `stop` is left out because the shipped workflow always refuses it for these services, and
/// `start` with it: a service which cannot be stopped this way has nothing to start.
const ACTIONS_OF_A_REQUIRED_SERVICE: &[&str] = &["restart", "enable", "disable"];

/// The only action of the agent when it is hosted by `tedge run all`.
///
/// It is the one action which never reaches an init system: the workflow moves to the
/// `restart-agent` step, where the agent stops itself and its runtime starts it again.
const ACTIONS_OF_A_HOSTED_AGENT: &[&str] = &["restart"];

/// The standard actions a hosted agent does not declare, the rest of [`ALL_ACTIONS`].
const OTHER_ACTIONS_OF_A_HOSTED_AGENT: &[&str] = &["start", "stop", "enable", "disable"];

/// How a thin-edge service is deployed, which decides what it is able to declare.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceDeployment {
    /// Started on its own, managed by an init unit of its own.
    OwnUnit,

    /// Hosted by `tedge run all`, sharing a process with the other components.
    ///
    /// No init unit manages such a service: the packaged units are stopped in that deployment,
    /// so that the supervisor can take the single-instance lock of each component.
    Hosted,
}

/// What a thin-edge service publishes about its own actions when it starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceActions {
    /// The actions declared, one `cmd/<action>` capability each.
    pub declared: &'static [&'static str],

    /// The actions withdrawn, by clearing the capability a previous deployment may have left.
    ///
    /// A capability is retained, so it outlives the service which published it, and moving a
    /// device to `tedge run all` would otherwise leave the actions of the previous deployment
    /// on show. Only the standard actions are ever withdrawn: a custom action is named by
    /// whoever wrote its workflow, so thin-edge neither knows it nor owns it.
    pub withdrawn: &'static [&'static str],
}

impl ServiceActions {
    /// A service thin-edge decides nothing for, which publishes nothing about its actions.
    pub const NONE: ServiceActions = ServiceActions {
        declared: &[],
        withdrawn: &[],
    };

    /// The given actions, with none of the standard ones withdrawn.
    pub const fn declaring(declared: &'static [&'static str]) -> Self {
        ServiceActions {
            declared,
            withdrawn: &[],
        }
    }
}

/// The actions a thin-edge service declares, and withdraws, on its own service entity at startup.
///
/// A service declares an action to say that a command posted on the matching `cmd/<action>`
/// topic is handled, so an action which could only ever fail is not declared: Cumulocity would
/// show it and let an operator create an operation which is refused every time.
///
/// This is what is offered, never what is enforced. A capability is a retained MQTT message
/// anyone can publish, and a command can be posted with no capability declared at all, so the
/// guards of the shipped workflows stay the only thing which refuses an action.
///
/// ```
/// use tedge_api::service_command::service_actions;
/// use tedge_api::service_command::ServiceDeployment::*;
///
/// assert_eq!(service_actions("tedge-agent", OwnUnit).declared, ["restart", "enable", "disable"]);
/// assert_eq!(service_actions("tedge-mapper-c8y", OwnUnit).declared, ["restart", "enable", "disable"]);
/// assert_eq!(service_actions("tedge-mapper-collectd", OwnUnit).declared.len(), 5);
///
/// assert_eq!(service_actions("tedge-agent", Hosted).declared, ["restart"]);
/// assert!(service_actions("tedge-mapper-c8y", Hosted).declared.is_empty());
/// assert_eq!(service_actions("tedge-mapper-c8y", Hosted).withdrawn.len(), 5);
/// ```
pub fn service_actions(service_name: &str, deployment: ServiceDeployment) -> ServiceActions {
    let is_agent = service_name == TEDGE_AGENT;
    let is_mapper = service_name.starts_with(MAPPER_PREFIX);

    match deployment {
        // Nothing else can be carried out on a hosted service: it has no unit of its own, so
        // every action going through an init system would act on a unit which is not what runs.
        // Which is also why what is not declared is withdrawn here and nowhere else: on a hosted
        // service, a standard action left by a previous deployment is one which cannot work.
        ServiceDeployment::Hosted if is_agent => ServiceActions {
            declared: ACTIONS_OF_A_HOSTED_AGENT,
            withdrawn: OTHER_ACTIONS_OF_A_HOSTED_AGENT,
        },
        ServiceDeployment::Hosted if is_mapper => ServiceActions {
            declared: &[],
            withdrawn: ALL_ACTIONS,
        },
        ServiceDeployment::Hosted => ServiceActions::NONE,

        // A mapper connected to no cloud takes no way of reporting anything away when it is
        // stopped, which is why the shipped `stop` workflow lets it be stopped.
        ServiceDeployment::OwnUnit if CLOUDLESS_MAPPERS.contains(&service_name) => {
            ServiceActions::declaring(ALL_ACTIONS)
        }

        // Every other mapper is what carries a command to its cloud, and the agent is what runs
        // the command. A user-defined mapper is one of them: the shipped workflow refuses to stop
        // any `tedge-mapper-<x>` but the two above, having no way to tell which cloud it serves.
        ServiceDeployment::OwnUnit if is_agent || is_mapper => {
            ServiceActions::declaring(ACTIONS_OF_A_REQUIRED_SERVICE)
        }

        // Any other service declares its own actions, thin-edge deciding nothing for it.
        ServiceDeployment::OwnUnit => ServiceActions::NONE,
    }
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
    #[test_case("dbus-:1.2-org.freedesktop.problems@0"; "a systemd unit name holding a colon")]
    fn accepted_service_names(name: &str) {
        assert!(validate_service_name(name).is_ok(), "{name}");
    }

    #[test_case("collectd stop"; "with a space")]
    #[test_case("collectd;reboot"; "with a shell separator")]
    #[test_case("../collectd"; "naming a path")]
    #[test_case("Nginx Web Server"; "a display name")]
    #[test_case("メインサービス"; "not written in ascii")]
    fn a_name_no_step_refuses_is_accepted(name: &str) {
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

    /// The agent runs the command, and a mapper carries it to its cloud, so neither can be
    /// stopped this way. Every one of them declares the same three actions.
    #[test_case("tedge-agent"; "the agent")]
    #[test_case("tedge-mapper-c8y"; "a c8y mapper")]
    #[test_case("tedge-mapper-az"; "an azure mapper")]
    #[test_case("tedge-mapper-aws"; "an aws mapper")]
    #[test_case("tedge-mapper-c8y-eu"; "a mapper with its own topic prefix")]
    #[test_case("tedge-mapper-my-flows"; "a user-defined mapper")]
    fn a_service_thin_edge_needs_declares_neither_start_nor_stop(name: &str) {
        let actions = service_actions(name, ServiceDeployment::OwnUnit);
        assert_eq!(actions.declared, ["restart", "enable", "disable"]);
        assert!(actions.withdrawn.is_empty());
    }

    /// Both are connected to no cloud, and the shipped `stop` workflow lets them be stopped.
    #[test_case("tedge-mapper-collectd")]
    #[test_case("tedge-mapper-local")]
    fn a_mapper_connected_to_no_cloud_declares_every_action(name: &str) {
        let actions = service_actions(name, ServiceDeployment::OwnUnit);
        assert_eq!(
            actions.declared,
            ["start", "stop", "restart", "enable", "disable"]
        );
        assert!(actions.withdrawn.is_empty());
    }

    /// The agent restarts itself, so its own restart never reaches an init system. Every other
    /// action of a hosted service would act on a unit which is not what runs.
    #[test]
    fn a_hosted_agent_declares_its_restart_and_nothing_else() {
        let actions = service_actions("tedge-agent", ServiceDeployment::Hosted);
        assert_eq!(actions.declared, ["restart"]);
        assert_eq!(actions.withdrawn, ["start", "stop", "enable", "disable"]);
    }

    #[test_case("tedge-mapper-c8y"; "a cloud mapper")]
    #[test_case("tedge-mapper-collectd"; "a mapper connected to no cloud")]
    #[test_case("tedge-mapper-local"; "the local mapper")]
    fn a_hosted_mapper_declares_no_action(name: &str) {
        let actions = service_actions(name, ServiceDeployment::Hosted);
        assert!(actions.declared.is_empty());
        assert_eq!(actions.withdrawn, ALL_ACTIONS);
    }

    /// A capability is retained, so moving a device to `tedge run all` would leave the actions of
    /// the previous deployment on show. Every standard action a hosted service does not declare
    /// is cleared, so the two lists together are the whole set, and neither holds the same action.
    #[test_case("tedge-agent"; "the agent")]
    #[test_case("tedge-mapper-c8y"; "a cloud mapper")]
    #[test_case("tedge-mapper-collectd"; "a mapper connected to no cloud")]
    fn a_hosted_service_accounts_for_every_standard_action(name: &str) {
        let actions = service_actions(name, ServiceDeployment::Hosted);

        let mut accounted: Vec<_> = actions
            .declared
            .iter()
            .chain(actions.withdrawn.iter())
            .collect();
        accounted.sort();

        let mut all: Vec<_> = ALL_ACTIONS.iter().collect();
        all.sort();

        assert_eq!(accounted, all);
    }

    /// Nothing is ever withdrawn from a service started on its own: an action declared by hand
    /// there is one an administrator added, next to a workflow guard they may have lifted.
    #[test_case("tedge-agent"; "the agent")]
    #[test_case("tedge-mapper-c8y"; "a cloud mapper")]
    #[test_case("tedge-mapper-collectd"; "a mapper connected to no cloud")]
    #[test_case("my-service"; "a service thin-edge does not ship")]
    fn a_service_on_its_own_unit_withdraws_nothing(name: &str) {
        assert!(service_actions(name, ServiceDeployment::OwnUnit)
            .withdrawn
            .is_empty());
    }

    /// thin-edge decides nothing for a service it does not ship, whichever the deployment.
    #[test_case(ServiceDeployment::OwnUnit)]
    #[test_case(ServiceDeployment::Hosted)]
    fn any_other_service_declares_its_own_actions(deployment: ServiceDeployment) {
        assert_eq!(
            service_actions("c8y-firmware-plugin", deployment),
            ServiceActions::NONE
        );
        assert_eq!(
            service_actions("my-service", deployment),
            ServiceActions::NONE
        );
    }
}
