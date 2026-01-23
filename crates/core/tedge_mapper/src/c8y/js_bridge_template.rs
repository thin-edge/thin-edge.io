//! JavaScript bridge template support for generating bridge rules dynamically.
//!
//! This module allows bridge rules to be defined using JavaScript, enabling
//! more expressive rule generation with arrays, spread operators, map/filter,
//! and conditional logic.

use camino::Utf8PathBuf;
use serde::Deserialize;
use serde_json::json;
use tedge_config::tedge_toml::ProfileName;
use tedge_config::TEdgeConfig;
use tedge_flows::JsonValue;
use tedge_flows::JsRuntime;
use tedge_flows::JsScript;
use tedge_flows::LoadError;
use tedge_mqtt_bridge::AuthMethod;
use tedge_mqtt_bridge::Direction;
use tedge_mqtt_bridge::InvalidBridgeRule;

/// Expanded bridge rule from JS template execution
#[derive(Debug)]
pub struct ExpandedBridgeRule {
    pub local_prefix: String,
    pub remote_prefix: String,
    pub direction: Direction,
    pub topic: String,
}

/// Error type for JS bridge template operations
#[derive(thiserror::Error, Debug)]
pub enum JsBridgeError {
    #[error("JS runtime error: {0}")]
    Runtime(#[from] LoadError),

    #[error("JSON parsing error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Invalid JS output: expected JSON string, got {0}")]
    InvalidOutput(String),

    #[error("Bridge rule error: {0}")]
    BridgeRule(#[from] InvalidBridgeRule),

    #[error("Config access error: {0}")]
    ConfigAccess(String),
}

/// JSON structure returned by JS bridge_config function
#[derive(Deserialize, Debug)]
struct JsBridgeConfig {
    local_prefix: Option<String>,
    remote_prefix: Option<String>,
    rule: Vec<JsBridgeRule>,
}

/// Individual rule from JS output
#[derive(Deserialize, Debug)]
struct JsBridgeRule {
    topic: String,
    direction: JsDirection,
    #[serde(default = "default_enabled")]
    enabled: bool,
    local_prefix: Option<String>,
    remote_prefix: Option<String>,
}

fn default_enabled() -> bool {
    true
}

/// Direction enum matching JS output
#[derive(Deserialize, Debug, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum JsDirection {
    Inbound,
    Outbound,
    Bidirectional,
}

impl From<JsDirection> for Direction {
    fn from(dir: JsDirection) -> Self {
        match dir {
            JsDirection::Inbound => Direction::Inbound,
            JsDirection::Outbound => Direction::Outbound,
            JsDirection::Bidirectional => Direction::Bidirectional,
        }
    }
}

/// Execute a JavaScript bridge template and return expanded rules.
///
/// The JS function should have the signature:
/// ```javascript
/// export function bridge_config(connection, config) {
///   return JSON.stringify({
///     local_prefix: "c8y/",
///     remote_prefix: "",
///     rule: [
///       { direction: "outbound", topic: "s/us/#", enabled: true },
///       // ...
///     ]
///   });
/// }
/// ```
pub async fn execute_js_bridge_template(
    js_source: &str,
    tedge_config: &TEdgeConfig,
    auth_method: AuthMethod,
    cloud_profile: Option<&ProfileName>,
) -> Result<Vec<ExpandedBridgeRule>, JsBridgeError> {
    // Create JS runtime
    let mut runtime = JsRuntime::try_new().await?;

    // Load the script
    let mut script = JsScript::new(
        "bridge_template".to_string(),
        Utf8PathBuf::from("bridge"),
        Utf8PathBuf::from("bridge/smartrest.js"),
    );
    runtime
        .load_script_literal(&mut script, js_source.as_bytes().to_vec())
        .await?;

    // Build connection object
    let connection_json = json!({
        "auth_method": auth_method.to_string()
    });

    // Build config object with relevant values
    let config_json = build_config_json(tedge_config, cloud_profile)?;

    // Convert to JsonValue for JS runtime
    let connection_arg: JsonValue = serde_json::Value::from(connection_json).into();
    let config_arg: JsonValue = config_json.into();

    // Call the bridge_config function
    let result = runtime
        .call_function("bridge_template", "bridge_config", vec![connection_arg, config_arg])
        .await?;

    // Parse the JSON string result
    let result_string = match result {
        JsonValue::String(s) => s,
        other => {
            return Err(JsBridgeError::InvalidOutput(format!("{other:?}")));
        }
    };

    let js_config: JsBridgeConfig = serde_json::from_str(&result_string)?;

    // Convert to ExpandedBridgeRule, filtering disabled rules
    let mut rules = Vec::new();
    for rule in js_config.rule {
        if !rule.enabled {
            continue;
        }

        let local_prefix = rule
            .local_prefix
            .or_else(|| js_config.local_prefix.clone())
            .unwrap_or_default();
        let remote_prefix = rule
            .remote_prefix
            .or_else(|| js_config.remote_prefix.clone())
            .unwrap_or_default();

        rules.push(ExpandedBridgeRule {
            local_prefix,
            remote_prefix,
            direction: rule.direction.into(),
            topic: rule.topic,
        });
    }

    Ok(rules)
}

/// Build a JSON object with config values relevant to bridge templates.
///
/// This creates a nested structure accessible in JS as:
/// - config.c8y.bridge.topic_prefix
/// - config.c8y.smartrest.templates
/// - config.c8y.smartrest1.templates
/// - config.c8y.mqtt_service.enabled
/// - config.c8y.mqtt_service.topics
fn build_config_json(
    tedge_config: &TEdgeConfig,
    cloud_profile: Option<&ProfileName>,
) -> Result<serde_json::Value, JsBridgeError> {
    let c8y_config = tedge_config
        .c8y_mapper_config(&cloud_profile)
        .map_err(|e| JsBridgeError::ConfigAccess(e.to_string()))?;

    // Extract SmartREST templates
    let smartrest_templates: Vec<&str> = c8y_config
        .cloud_specific
        .smartrest
        .templates
        .0
        .iter()
        .map(|s| s.as_str())
        .collect();

    let smartrest1_templates: Vec<&str> = c8y_config
        .cloud_specific
        .smartrest1
        .templates
        .0
        .iter()
        .map(|s| s.as_str())
        .collect();

    // Extract MQTT service config
    let mqtt_service_enabled = c8y_config.cloud_specific.mqtt_service.enabled;
    let mqtt_service_topics: Vec<&str> = c8y_config
        .cloud_specific
        .mqtt_service
        .topics
        .0
        .iter()
        .map(|s| s.as_str())
        .collect();

    // Build the nested JSON structure
    Ok(json!({
        "c8y": {
            "bridge": {
                "topic_prefix": c8y_config.bridge.topic_prefix.as_str()
            },
            "smartrest": {
                "templates": smartrest_templates
            },
            "smartrest1": {
                "templates": smartrest1_templates
            },
            "mqtt_service": {
                "enabled": mqtt_service_enabled,
                "topics": mqtt_service_topics
            }
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio;

    #[tokio::test]
    async fn test_simple_js_template() {
        let js_source = r#"
            export function bridge_config(connection, config) {
                return JSON.stringify({
                    local_prefix: "c8y/",
                    remote_prefix: "",
                    rule: [
                        { direction: "outbound", topic: "s/us/#", enabled: true },
                    ]
                });
            }
        "#;

        let config = TEdgeConfig::load_toml_str("");
        let rules = execute_js_bridge_template(js_source, &config, AuthMethod::Certificate, None)
            .await
            .unwrap();

        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].topic, "s/us/#");
        assert_eq!(rules[0].local_prefix, "c8y/");
        assert_eq!(rules[0].remote_prefix, "");
        assert!(matches!(rules[0].direction, Direction::Outbound));
    }

    #[tokio::test]
    async fn test_conditional_rules_based_on_auth_method() {
        let js_source = r#"
            export function bridge_config(connection, config) {
                const rules = [
                    { direction: "outbound", topic: "s/us/#", enabled: true },
                ];

                if (connection.auth_method === "certificate") {
                    rules.push({ direction: "inbound", topic: "s/dat", enabled: true });
                }

                return JSON.stringify({
                    local_prefix: "c8y/",
                    remote_prefix: "",
                    rule: rules
                });
            }
        "#;

        let config = TEdgeConfig::load_toml_str("");

        // Test with certificate auth
        let rules =
            execute_js_bridge_template(js_source, &config, AuthMethod::Certificate, None)
                .await
                .unwrap();
        assert_eq!(rules.len(), 2);

        // Test with password auth
        let rules = execute_js_bridge_template(js_source, &config, AuthMethod::Password, None)
            .await
            .unwrap();
        assert_eq!(rules.len(), 1);
    }

    #[tokio::test]
    async fn test_disabled_rules_are_filtered() {
        let js_source = r#"
            export function bridge_config(connection, config) {
                return JSON.stringify({
                    local_prefix: "c8y/",
                    remote_prefix: "",
                    rule: [
                        { direction: "outbound", topic: "s/us/#", enabled: true },
                        { direction: "outbound", topic: "disabled/topic", enabled: false },
                        { direction: "inbound", topic: "s/ds", enabled: true },
                    ]
                });
            }
        "#;

        let config = TEdgeConfig::load_toml_str("");
        let rules = execute_js_bridge_template(js_source, &config, AuthMethod::Certificate, None)
            .await
            .unwrap();

        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].topic, "s/us/#");
        assert_eq!(rules[1].topic, "s/ds");
    }

    #[tokio::test]
    async fn test_per_rule_prefix_override() {
        let js_source = r#"
            export function bridge_config(connection, config) {
                return JSON.stringify({
                    local_prefix: "default/",
                    remote_prefix: "",
                    rule: [
                        { direction: "outbound", topic: "topic1", enabled: true },
                        { direction: "outbound", topic: "topic2", enabled: true, local_prefix: "custom/" },
                    ]
                });
            }
        "#;

        let config = TEdgeConfig::load_toml_str("");
        let rules = execute_js_bridge_template(js_source, &config, AuthMethod::Certificate, None)
            .await
            .unwrap();

        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].local_prefix, "default/");
        assert_eq!(rules[1].local_prefix, "custom/");
    }

    #[tokio::test]
    async fn test_array_spread_and_map() {
        let js_source = r#"
            export function bridge_config(connection, config) {
                const modes = ["s", "t", "q", "c"];
                const rules = modes.map(mode => ({
                    direction: "outbound",
                    topic: mode + "/us/#",
                    enabled: true
                }));

                return JSON.stringify({
                    local_prefix: "c8y/",
                    remote_prefix: "",
                    rule: rules
                });
            }
        "#;

        let config = TEdgeConfig::load_toml_str("");
        let rules = execute_js_bridge_template(js_source, &config, AuthMethod::Certificate, None)
            .await
            .unwrap();

        assert_eq!(rules.len(), 4);
        assert_eq!(rules[0].topic, "s/us/#");
        assert_eq!(rules[1].topic, "t/us/#");
        assert_eq!(rules[2].topic, "q/us/#");
        assert_eq!(rules[3].topic, "c/us/#");
    }

    #[tokio::test]
    async fn test_config_access() {
        let js_source = r#"
            export function bridge_config(connection, config) {
                const templates = config.c8y?.smartrest?.templates || [];
                const rules = templates.map(t => ({
                    direction: "inbound",
                    topic: "s/dc/" + t,
                    enabled: true
                }));

                return JSON.stringify({
                    local_prefix: config.c8y?.bridge?.topic_prefix + "/",
                    remote_prefix: "",
                    rule: rules
                });
            }
        "#;

        let config = TEdgeConfig::load_toml_str(
            r#"
[c8y]
smartrest.templates = ["template1", "template2"]
"#,
        );

        let rules = execute_js_bridge_template(js_source, &config, AuthMethod::Certificate, None)
            .await
            .unwrap();

        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].topic, "s/dc/template1");
        assert_eq!(rules[1].topic, "s/dc/template2");
        assert_eq!(rules[0].local_prefix, "c8y/");
    }

    #[tokio::test]
    async fn test_all_directions() {
        let js_source = r#"
            export function bridge_config(connection, config) {
                return JSON.stringify({
                    local_prefix: "c8y/",
                    remote_prefix: "",
                    rule: [
                        { direction: "inbound", topic: "in/topic", enabled: true },
                        { direction: "outbound", topic: "out/topic", enabled: true },
                        { direction: "bidirectional", topic: "bidir/topic", enabled: true },
                    ]
                });
            }
        "#;

        let config = TEdgeConfig::load_toml_str("");
        let rules = execute_js_bridge_template(js_source, &config, AuthMethod::Certificate, None)
            .await
            .unwrap();

        assert_eq!(rules.len(), 3);
        assert!(matches!(rules[0].direction, Direction::Inbound));
        assert!(matches!(rules[1].direction, Direction::Outbound));
        assert!(matches!(rules[2].direction, Direction::Bidirectional));
    }
}
