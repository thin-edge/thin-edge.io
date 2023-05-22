#[doc(inline)]
#[doc = include_str!("define_tedge_config_docs.md")]
pub use tedge_config_macros_macro::define_tedge_config;

pub use all_or_nothing::*;
#[doc(hidden)]
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
