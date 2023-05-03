#[doc(inline)]
pub use tedge_config_macros_macro::define_tedge_config;

pub use all_or_nothing::*;
pub use connect_url::*;
pub use default::*;
pub use doku_aliases::*;
pub use option::*;

mod all_or_nothing;
mod connect_url;
mod default;
mod doku_aliases;
#[cfg(doc)]
pub mod example;
mod option;
