//! A field type without an `AppendRemoveItem` impl is reported against the
//! type in the schema, not against generated registry code

use tedge_config_engine::*;

tedge_config_engine_macro::define_config! {
    Test {
        mqtt: {
            /// Separator character
            separator: char,
        },
    }
}

fn main() {}
