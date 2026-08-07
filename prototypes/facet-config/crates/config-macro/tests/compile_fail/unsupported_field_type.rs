//! A field type without an `AppendRemoveItem` impl is reported against the
//! type in the schema, not against generated registry code

use facet_config_runtime::*;

facet_config_macro::define_config! {
    Test {
        mqtt: {
            /// Separator character
            separator: char,
        },
    }
}

fn main() {}
