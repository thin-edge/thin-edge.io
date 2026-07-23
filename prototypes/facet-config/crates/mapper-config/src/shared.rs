//! Configuration shared between mapper schemas.
//!
//! `MapperDevice` is mounted into each mapper schema with
//! `device: extern MapperDeviceConfig`. Its `from_key_via` source key is
//! written relative to this schema (`cert_path`, not `device.cert_path`);
//! mounting remaps it into the mounting schema's key space. The `from_root`
//! keys name keys of the root tedge config and are never remapped.

use facet_config_runtime::*;

facet_config_macro::define_config! {
    MapperDevice {
        /// Unique device identifier for this mapper
        #[tedge_config(default(from_key_via(
            key = "cert_path",
            function = "certificate_common_name"
        )))]
        id: String,

        /// Path to the device certificate for this mapper
        #[tedge_config(default(from_root = "device.cert_path"))]
        cert_path: camino::Utf8PathBuf,

        /// Path to the device private key for this mapper
        #[tedge_config(default(from_root = "device.key_path"))]
        key_path: camino::Utf8PathBuf,
    }
}
