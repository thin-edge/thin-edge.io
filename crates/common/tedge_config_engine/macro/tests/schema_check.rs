//! Exercises `check_schema` — the check `define_config!` runs from a generated
//! test — against schemas that declare defaults and examples incorrectly.
//!
//! Every schema here is deliberately invalid, so each one opts out of the
//! generated test and calls `check_schema` explicitly instead.

use tedge_config_engine::*;

const CONFIG_DIR: &str = "/etc/tedge";

mod valid {
    use super::*;

    tedge_config_engine_macro::define_config! {
        Valid {
            mqtt: {
                /// MQTT broker port
                #[tedge_config(example = "1883", example = "8883", default(value = "1883"))]
                port: u16,

                /// Port to fall back to
                #[tedge_config(default(from_key = "mqtt.port"))]
                fallback_port: u16,

                /// Port derived from the broker port
                #[tedge_config(default(from_key_via(key = "mqtt.port", function = "next_port")))]
                tls_port: u16,
            },

            device: {
                /// Certificate path
                #[tedge_config(default(from_config_dir = "device-certs/tedge-certificate.pem"))]
                cert_path: String,

                /// Freshly generated identifier
                #[tedge_config(default(function = "generated_id"))]
                stamp: String,

                /// Identifier, defaulting to the certificate path
                #[tedge_config(default(from_optional_key = "device.cert_path"))]
                id: String,
            },
        }
    }

    #[test]
    fn a_schema_whose_defaults_and_examples_are_all_valid_passes() {
        assert_eq!(
            check_schema::<ValidConfig>(std::path::Path::new(CONFIG_DIR)),
            Ok(())
        );
    }
}

mod invalid_default {
    use super::*;

    tedge_config_engine_macro::define_config! {
        #[tedge_config(skip_generated_test)]
        InvalidDefault {
            mqtt: {
                /// A port whose default is not a port
                #[tedge_config(default(value = "188x"))]
                port: u16,
            },
        }
    }

    #[test]
    fn a_default_that_is_not_a_valid_value_is_reported_as_a_bug() {
        assert_eq!(
            problems::<InvalidDefaultConfig>(),
            [
                "The built-in default for 'mqtt.port' is not a valid value: '188x' — \
              failed to parse \"188x\" as u16 at mqtt. This is a bug in thin-edge.io."
            ]
        );
    }
}

mod inherited_invalid_default {
    use super::*;

    tedge_config_engine_macro::define_config! {
        #[tedge_config(skip_generated_test)]
        InheritedInvalidDefault {
            mqtt: {
                /// A name, whose default is valid for a name but not for a port
                #[tedge_config(default(value = "188x"))]
                name: String,

                /// A port inheriting a default it cannot parse
                #[tedge_config(default(from_key = "mqtt.name"))]
                tls_port: u16,
            },
        }
    }

    #[test]
    fn an_inherited_invalid_default_names_the_key_it_came_from() {
        assert_eq!(
            problems::<InheritedInvalidDefaultConfig>(),
            [
                "'mqtt.tls_port' is not set, so it falls back to 'mqtt.name', whose built-in \
              default '188x' is not valid: failed to parse \"188x\" as u16 at mqtt. \
              This is a bug in thin-edge.io."
            ]
        );
    }
}

mod invalid_example {
    use super::*;

    tedge_config_engine_macro::define_config! {
        #[tedge_config(skip_generated_test)]
        InvalidExample {
            mqtt: {
                /// A port with an example that is not a port
                #[tedge_config(example = "1883", example = "not-a-port")]
                port: u16,
            },
        }
    }

    #[test]
    fn only_the_example_that_is_not_a_valid_value_is_reported() {
        assert_eq!(
            problems::<InvalidExampleConfig>(),
            ["mqtt.port: example 'not-a-port' is not a valid value: \
              failed to parse \"not-a-port\" as u16 at mqtt"]
        );
    }
}

mod unknown_source_key {
    use super::*;

    tedge_config_engine_macro::define_config! {
        #[tedge_config(skip_generated_test)]
        UnknownSourceKey {
            mqtt: {
                /// A port
                port: u16,

                /// A port falling back to a misspelled key
                #[tedge_config(default(from_optional_key = "mqtt.prot"))]
                tls_port: u16,
            },
        }
    }

    #[test]
    #[should_panic(
        expected = "The default for 'mqtt.tls_port' falls back to 'mqtt.prot', which is not a key in this schema"
    )]
    fn a_fallback_to_a_misspelled_key_is_rejected_when_the_manager_is_built() {
        ConfigManager::from_schema::<UnknownSourceKeyConfig>(std::path::Path::new(CONFIG_DIR));
    }
}

mod several_problems {
    use super::*;

    tedge_config_engine_macro::define_config! {
        #[tedge_config(skip_generated_test)]
        SeveralProblems {
            mqtt: {
                /// A port with both a bad default and a bad example
                #[tedge_config(example = "not-a-port", default(value = "188x"))]
                port: u16,

                /// Another port with a bad default
                #[tedge_config(default(value = "-1"))]
                tls_port: u16,
            },
        }
    }

    #[test]
    fn every_problem_is_reported_rather_than_only_the_first() {
        assert_eq!(
            problems::<SeveralProblemsConfig>(),
            [
                "mqtt.port: example 'not-a-port' is not a valid value: \
                 failed to parse \"not-a-port\" as u16 at mqtt",
                "The built-in default for 'mqtt.port' is not a valid value: '188x' — \
                 failed to parse \"188x\" as u16 at mqtt. This is a bug in thin-edge.io.",
                "The built-in default for 'mqtt.tls_port' is not a valid value: '-1' — \
                 failed to parse \"-1\" as u16 at mqtt. This is a bug in thin-edge.io.",
            ]
        );
    }
}

/// The problems `check_schema` reports for a schema that is expected to have some
fn problems<C: ConfigSchema + BuildFromDto>() -> Vec<String> {
    check_schema::<C>(std::path::Path::new(CONFIG_DIR))
        .expect_err("schema should have been rejected")
        .problems()
        .to_vec()
}

fn next_port(port: &str) -> Result<Option<u16>, String> {
    let port: u16 = port.parse().map_err(|e| format!("not a port: {e}"))?;
    Ok(Some(port + 1))
}

fn generated_id() -> String {
    "generated".into()
}
