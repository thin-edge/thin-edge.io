use serde::Deserialize;

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
}

#[derive(Deserialize, Debug, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct InitConfigToml {
    name: String,
    is_available: Vec<String>,
    restart: Vec<String>,
    stop: Vec<String>,
    start: Option<Vec<String>>,
    enable: Vec<String>,
    disable: Vec<String>,
    is_active: Vec<String>,
}

impl From<InitConfigToml> for InitConfig {
    fn from(value: InitConfigToml) -> Self {
        Self {
            name: value.name,
            is_available: value.is_available,
            start: value.start.unwrap_or(value.restart.clone()),
            restart: value.restart,
            stop: value.stop,
            enable: value.enable,
            disable: value.disable,
            is_active: value.is_active,
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
        }
    }
}
