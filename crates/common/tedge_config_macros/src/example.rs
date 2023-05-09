//! An example invocation of [define_tedge_config] to demonstrate what
//! it expands to in `cargo doc` output.
use super::*;
use camino::Utf8PathBuf;
use std::path::PathBuf;

#[derive(thiserror::Error, Debug)]
/// Not macro generated! An error that can be encountered when reading values
/// from the configuration
///
/// As custom logic (e.g. for read-only values) needs to interact with this,
/// this is left to the consuming module to define. It must include a case
/// with `#[from] ConfigNotSet`.
pub enum ReadError {
    #[error(transparent)]
    ConfigNotSet(#[from] ConfigNotSet),
}

define_tedge_config! {
    device: {
        #[doku(as = "PathBuf")]
        root_cert_path: Utf8PathBuf,

        #[doku(as = "PathBuf")]
        #[tedge_config(default(from_path = "device.root_cert_path"))]
        root_cert_path2: Utf8PathBuf,

        #[tedge_config(rename = "type")]
        ty: String,
    }
}
