//! An example invocation of [define_tedge_config] to demonstrate what
//! it expands to in `cargo doc` output.
use super::*;
use camino::Utf8PathBuf;
use std::path::PathBuf;

define_tedge_config! {
    device: {
        #[doku(as = "PathBuf")]
        root_cert_path: Utf8PathBuf,
        #[doku(as = "PathBuf")]
        #[tedge_config(default(from_path = "device.root_cert_path"))]
        root_cert_path2: Utf8PathBuf,
    }
}
