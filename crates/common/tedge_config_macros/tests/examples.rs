use camino::Utf8PathBuf;
use tedge_config_macros::*;

#[derive(thiserror::Error, Debug)]
pub enum ReadError {
    #[error(transparent)]
    ConfigNotSet(#[from] ConfigNotSet),
}

// The macro invocation generates tests of its own for each example value
define_tedge_config! {
    device: {
        #[tedge_config(example = "/test/cert/path")]
        #[tedge_config(example = "/test/cert/path2")]
        #[doku(as = "std::path::PathBuf")]
        root_cert_path: Utf8PathBuf,
    }
}
