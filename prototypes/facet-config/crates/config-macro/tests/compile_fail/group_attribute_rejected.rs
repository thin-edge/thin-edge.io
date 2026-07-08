//! Groups accept no `tedge_config` attributes

facet_config_macro::define_config! {
    Test {
        #[tedge_config(multi)]
        c8y: {
            /// Cloud URL
            url: String,
        },
    }
}

fn main() {}
