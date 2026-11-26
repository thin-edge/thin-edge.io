//! A default source listed after one that always gives a value is rejected,
//! as it would never be tried

tedge_config_engine_macro::define_config! {
    Test {
        device: {
            /// Device identifier
            #[tedge_config(default(value = "my-device", from_root = "device.id"))]
            id: String,
        },
    }
}

fn main() {}
