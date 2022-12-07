pub mod file;
pub mod fs;
pub mod paths;
pub mod signals;
pub mod timers;

#[cfg(feature = "fs-notify")]
pub mod notify;

// We add a `tedge_derive` prefix to the import of `serde_other`
// so that importing `serde_other` hints that it is a derive macro.
#[cfg(feature = "tedge-derive")]
pub mod tedge_derive {
    pub use tedge_derive::serde_other;
}
