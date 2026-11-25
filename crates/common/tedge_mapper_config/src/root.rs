//! The tables of the root `tedge.toml` that mapper schemas fall back to

use camino::Utf8PathBuf;
use std::num::NonZeroU16;
use tedge_config_engine::*;

use crate::absolute_path::AbsolutePath;
use crate::cert::certificate_common_name;
use crate::cryptoki::Cryptoki;

tedge_config_engine_macro::define_config! {
    Root {
        device: {
            /// Identifier of the device within the fleet. It must be globally
            /// unique and is derived from the device certificate.
            #[tedge_config(
                example = "Raspberrypi-4d18303a-6d3a-11eb-b1a6-175f6bb72665",
                exposable,
                default(from_key_via(key = "device.cert_path", function = "certificate_common_name"))
            )]
            id: String,

            /// Path where the device's private key is stored
            #[tedge_config(
                example = "/etc/tedge/device-certs/tedge-private-key.pem",
                default(from_config_dir = "device-certs/tedge-private-key.pem")
            )]
            key_path: AbsolutePath,

            /// Path where the device's certificate is stored
            #[tedge_config(
                example = "/etc/tedge/device-certs/tedge-certificate.pem",
                default(from_config_dir = "device-certs/tedge-certificate.pem")
            )]
            cert_path: AbsolutePath,

            /// Path where the device's certificate signing request is stored
            #[tedge_config(
                example = "/etc/tedge/device-certs/tedge.csr",
                default(from_config_dir = "device-certs/tedge.csr")
            )]
            csr_path: AbsolutePath,

            /// A PKCS#11 URI of the private key.
            ///
            /// See RFC #7512.
            #[tedge_config(example = "pkcs11:token=my-pkcs11-token;object=my-key")]
            key_uri: String,

            /// User PIN value for logging into the PKCS#11 token provided by the consumer.
            ///
            /// This differs from cryptoki.pin in that cryptoki.pin is used by PKCS#11 provider, e.g. tedge-p11-server as a
            /// default PIN for all tokens, but device.key_pin is the PIN provided by the consumer (tedge) with a given
            /// `key_uri`.
            ///
            /// In practice, this can be used to define separate keys and separate PINs for different connection profiles.
            #[tedge_config(example = "123456", example = "my-pin")]
            key_pin: String,

            cryptoki: {
                /// Whether to use a Hardware Security Module for authenticating the MQTT connection with the cloud.
                ///
                /// "off" to not use the HSM, "module" to use the provided cryptoki dynamic module, "socket" to access the
                /// HSM via tedge-p11-server signing service.
                #[tedge_config(default(value = "off"), example = "off", example = "module", example = "socket")]
                mode: Cryptoki,

                /// A path to the PKCS#11 module used for interaction with the HSM.
                ///
                /// Needs to be set when `device.cryptoki.mode` is set to `module`
                #[tedge_config(example = "/usr/lib/x86_64-linux-gnu/opensc-pkcs11.so")]
                module_path: AbsolutePath,

                /// A default User PIN value for logging into the PKCS11 token.
                ///
                /// May be overridden on a per-key basis using device.key_pin config setting.
                #[tedge_config(example = "123456", default(value = "123456"))]
                pin: String,

                /// A URI of the token/object to be used by tedge-p11-server.
                ///
                /// If set, tedge-p11-server will by default use this URI to select a key for signing if a client does not
                /// provide its URI in the request. If the client provides the URI, then the attributes of this server URI
                /// will be used as a base onto which client-provided URI attributes will be appended, potentially limiting
                /// the scope of keys or tokens that can be used by the clients.
                ///
                /// For example, if `cryptoki.uri=pkcs11:token=token1` and `device.key_uri=pkcs11:token2;object=key1`,
                /// `tedge-p11-server` will use URI `pkcs11:token1;object=key1`.
                ///
                /// For more information about PKCS11 URIs, see RFC7512.
                #[tedge_config(example = "pkcs11:token=my-pkcs11-token;object=my-key")]
                uri: String,

                /// A path to the tedge-p11-server socket.
                ///
                /// Needs to be set when `device.cryptoki.mode` is set to `socket`
                #[tedge_config(
                    default(value = "/run/tedge-p11-server/tedge-p11-server.sock"),
                    example = "/run/tedge-p11-server/tedge-p11-server.sock"
                )]
                socket_path: Utf8PathBuf,

                /// Number of object handles to request per PKCS#11 `C_FindObjects` call.
                ///
                /// A batch size of 1 improves compatibility with PKCS#11 modules that
                /// mishandle larger batches. Increase it only for a module known to benefit.
                #[tedge_config(example = "1", example = "10", default(value = "1"))]
                find_objects_batch_size: NonZeroU16,
            },

            /// The default device type
            #[tedge_config(example = "thin-edge.io", default(value = "thin-edge.io"), rename = "type", exposable)]
            ty: String,
        },
    }
}
