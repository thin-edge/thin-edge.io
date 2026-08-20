//! Groups accept no `tedge_config` attributes

tedge_config_engine_macro::define_config! {
    Test {
        #[tedge_config(multi)]
        c8y: {
            /// Cloud URL
            url: String,
        },
    }
}

fn main() {}
