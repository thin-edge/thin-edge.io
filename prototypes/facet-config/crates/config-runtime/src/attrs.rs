//! Custom facet attributes for thin-edge config fields.
//!
//! These are placed on DTO fields by `define_config!` and read at runtime via
//! facet's reflection API, replacing explicit codegen for read-only markers,
//! deprecated key aliases, and example values.

pub const NS: &str = "tedge";

facet::define_attr_grammar! {
    ns "tedge";
    crate_path ::facet_config_runtime;

    /// Thin-edge config field attributes.
    pub enum Attr {
        /// Marks a field as read-only (cannot be changed via `config set`).
        Readonly,

        /// Maps a deprecated key name to this field's canonical key.
        DeprecatedKey(&'static str),

        /// An example value shown in `config list` output.
        Example(&'static str),
    }
}
