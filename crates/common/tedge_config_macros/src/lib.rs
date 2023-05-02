#[doc(inline)]
pub use tedge_config_macros_macro::define_tedge_config;

pub use connect_url::*;
pub use default::*;
pub use doku_aliases::*;
pub use option::*;
mod connect_url;
mod default;
mod doku_aliases;
mod option;
