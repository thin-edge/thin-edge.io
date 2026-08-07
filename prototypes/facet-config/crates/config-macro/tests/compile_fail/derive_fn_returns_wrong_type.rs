//! A `from_key_via` function must return the field's own type; the error
//! should point at the `function` attribute value

use facet_config_runtime::*;

facet_config_macro::define_config! {
    Test {
        mqtt: {
            /// MQTT broker port
            #[tedge_config(default(value = "1883"))]
            port: u16,

            /// TLS port derived from the plain MQTT port
            #[tedge_config(default(from_key_via(key = "mqtt.port", function = "next_port")))]
            tls_port: u16,
        },
    }
}

fn next_port(port: &str) -> Result<Option<String>, String> {
    Ok(Some(port.to_owned()))
}

fn main() {}
