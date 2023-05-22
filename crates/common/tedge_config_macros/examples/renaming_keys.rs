use camino::Utf8PathBuf;
use std::path::PathBuf;
use tedge_config_macros::*;

#[derive(thiserror::Error, Debug)]
pub enum ReadError {
    #[error(transparent)]
    ConfigNotSet(#[from] ConfigNotSet),
    #[error("Something went wrong: {0}")]
    GenericError(String),
}

define_tedge_config! {
    device: {
        // Fields and groups can be renamed using #[tedge_config(rename)]
        // this is useful
        #[tedge_config(rename = "type")]
        ty: String,

    },

    mqtt: {
        client: {
            auth: {
                // Where we have renamed a field, we can use
                // #[tedge_config(deprecated_name)] to create an alias that both serde and `tedge config`
                // will understand
                #[tedge_config(deprecated_name = "cafile")]
                #[doku(as = "PathBuf")]
                ca_file: Utf8PathBuf,
            }
        },

        bind: {
            // If we've moved a field, we can use
            // #[tedge_config(deprecated_key)] to provide an alternative key for
            // `tedge config` to accept. NB: this changes necessitates a toml
            // migration as we cannot use serde aliases for a structural change
            // like this
            #[tedge_config(deprecated_key = "mqtt.port")]
            port: u16,
        }
    }
}

fn main() {
    let parsed_deprecated_key = "mqtt.port".parse::<ReadableKey>().unwrap();
    let parsed_current_key = "mqtt.bind.port".parse::<ReadableKey>().unwrap();
    assert_eq!(parsed_deprecated_key, parsed_current_key);
    assert_eq!(parsed_deprecated_key.as_str(), "mqtt.bind.port");

    let parsed_deprecated_key = "mqtt.client.auth.cafile".parse::<WritableKey>().unwrap();
    let parsed_current_key = "mqtt.client.auth.ca_file".parse::<WritableKey>().unwrap();
    assert_eq!(parsed_deprecated_key, parsed_current_key);
    assert_eq!(parsed_deprecated_key.as_str(), "mqtt.client.auth.ca_file")
}
