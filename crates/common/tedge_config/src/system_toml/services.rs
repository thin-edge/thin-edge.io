use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize, Debug, Eq, PartialEq)]
#[serde(from = "InitConfigToml")]
pub struct InitConfig {
    pub name: String,
    pub is_available: Vec<String>,
    pub restart: Vec<String>,
    pub stop: Vec<String>,
    pub start: Vec<String>,
    pub enable: Vec<String>,
    pub disable: Vec<String>,
    pub is_active: Vec<String>,

    /// Action templates defined in `[init]` beyond the ones above.
    ///
    /// A custom action is the same kind of value as `restart`: an argv list with a `{}`
    /// placeholder for the service name. Since `name` and the state-query templates are
    /// named fields, they can never end up here, which is what keeps them reserved.
    pub custom_actions: BTreeMap<String, Vec<String>>,
}

impl InitConfig {
    /// The argv template for a service action, or `None` if this init system has none.
    ///
    /// Only templates that act on a service are actions. `name`, `is_available` and
    /// `is_active` are not, and are never returned here.
    pub fn action(&self, action: &str) -> Option<&[String]> {
        match action {
            "start" => Some(&self.start),
            "stop" => Some(&self.stop),
            "restart" => Some(&self.restart),
            "enable" => Some(&self.enable),
            "disable" => Some(&self.disable),
            other => self.custom_actions.get(other).map(Vec::as_slice),
        }
    }

    /// Every action this init system can run, sorted.
    pub fn action_names(&self) -> Vec<&str> {
        let mut names = vec!["disable", "enable", "restart", "start", "stop"];
        names.extend(self.custom_actions.keys().map(String::as_str));
        names.sort_unstable();
        names
    }
}

/// Deserialization proxy for [`InitConfig`].
///
/// This does not use `deny_unknown_fields`: an unknown key is a custom action template,
/// collected by the flattened map. Serde does not combine `deny_unknown_fields` with
/// `flatten`, so a misspelled known key is read as a custom action rather than rejected.
/// `GeneralServiceManager` logs the actions it parsed, and `tedge service` lists them when
/// it rejects an unsupported action, so a typo stays discoverable.
#[derive(Deserialize, Debug, Eq, PartialEq)]
struct InitConfigToml {
    name: String,
    is_available: Vec<String>,
    restart: Vec<String>,
    stop: Vec<String>,
    start: Option<Vec<String>>,
    enable: Vec<String>,
    disable: Vec<String>,
    is_active: Vec<String>,

    #[serde(flatten)]
    custom_actions: BTreeMap<String, Vec<String>>,
}

impl From<InitConfigToml> for InitConfig {
    fn from(value: InitConfigToml) -> Self {
        if value.start.is_none() {
            tracing::debug!(
                "No 'start' template in [init]: falling back to the 'restart' template. \
                 A misspelled key is read as a custom action, not as 'start'."
            );
        }
        Self {
            name: value.name,
            is_available: value.is_available,
            start: value.start.unwrap_or(value.restart.clone()),
            restart: value.restart,
            stop: value.stop,
            enable: value.enable,
            disable: value.disable,
            is_active: value.is_active,
            custom_actions: value.custom_actions,
        }
    }
}

impl Default for InitConfig {
    fn default() -> Self {
        Self {
            name: "systemd".to_string(),
            is_available: vec!["/bin/systemctl".into(), "--version".into()],
            restart: vec!["/bin/systemctl".into(), "restart".into(), "{}".into()],
            stop: vec!["/bin/systemctl".into(), "stop".into(), "{}".into()],
            start: vec!["/bin/systemctl".into(), "start".into(), "{}".into()],
            enable: vec!["/bin/systemctl".into(), "enable".into(), "{}".into()],
            disable: vec!["/bin/systemctl".into(), "disable".into(), "{}".into()],
            is_active: vec!["/bin/systemctl".into(), "is-active".into(), "{}".into()],
            custom_actions: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(input: &str) -> InitConfig {
        toml::from_str(input).unwrap()
    }

    const KNOWN_KEYS: &str = r#"
        name = "systemd"
        is_available = ["/bin/systemctl", "--version"]
        restart = ["/bin/systemctl", "restart", "{}"]
        stop = ["/bin/systemctl", "stop", "{}"]
        start = ["/bin/systemctl", "start", "{}"]
        enable = ["/bin/systemctl", "enable", "{}"]
        disable = ["/bin/systemctl", "disable", "{}"]
        is_active = ["/bin/systemctl", "is-active", "{}"]
    "#;

    #[test]
    fn known_keys_only_leaves_no_custom_action() {
        let config = parse(KNOWN_KEYS);
        assert!(config.custom_actions.is_empty());
        assert_eq!(
            config.action_names(),
            ["disable", "enable", "restart", "start", "stop"]
        );
    }

    #[test]
    fn an_unknown_key_becomes_a_custom_action() {
        let input = format!(r#"{KNOWN_KEYS} reload = ["/bin/systemctl", "reload", "{{}}"]"#);
        let config = parse(&input);

        assert_eq!(
            config.action("reload"),
            Some(
                ["/bin/systemctl", "reload", "{}"]
                    .map(String::from)
                    .as_slice()
            )
        );
        assert_eq!(
            config.action_names(),
            ["disable", "enable", "reload", "restart", "start", "stop"]
        );
    }

    #[test]
    fn known_actions_are_looked_up_by_name() {
        let config = parse(KNOWN_KEYS);
        for action in ["start", "stop", "restart", "enable", "disable"] {
            assert!(
                config.action(action).is_some(),
                "{action} should be an action"
            );
        }
    }

    #[test]
    fn reserved_keys_are_not_actions() {
        let config = parse(KNOWN_KEYS);
        for reserved in ["name", "is_available", "is_active"] {
            assert_eq!(config.action(reserved), None, "{reserved} is not an action");
        }
    }

    #[test]
    fn an_undefined_action_is_absent() {
        assert_eq!(parse(KNOWN_KEYS).action("reload"), None);
    }
}
