//! An unknown `tedge_config` attribute on a field is rejected

facet_config_macro::define_config! {
    Test {
        mqtt: {
            /// MQTT broker port
            #[tedge_config(read_only)]
            port: u16,
        },
    }
}

fn main() {}
