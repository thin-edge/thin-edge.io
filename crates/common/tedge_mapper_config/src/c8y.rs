//! The schema of a Cumulocity mapper's `mapper.toml`, matching the `c8y`
//! table of tedge_config

use tedge_config_engine::*;

use crate::absolute_path::AbsolutePath;
use crate::c8y_auth_method::C8yAuthMethod;
use crate::cloud_type::CloudType;
use crate::connect_url::ConnectUrl;
use crate::templates_set::TemplatesSet;

tedge_config_engine_macro::define_config! {
    C8yMapper {
        /// The built-in cloud integration this mapper uses: c8y, az or aws
        cloud_type: CloudType,

        /// Endpoint URL of Cumulocity tenant
        #[tedge_config(example = "your-tenant.cumulocity.com", exposable)]
        url: ConnectUrl,

        /// The path where Cumulocity root certificate(s) are stored
        ///
        /// The value can be a directory path as well as the path of the certificate file.
        #[tedge_config(
            example = "/etc/tedge/c8y-trusted-root-certificates.pem",
            default(value = "/etc/ssl/certs")
        )]
        root_cert_path: AbsolutePath,

        /// The authentication method used to connect Cumulocity
        ///
        /// In the auto mode, basic auth is used if c8y.credentials_path is set
        #[tedge_config(
            example = "certificate",
            example = "basic",
            example = "auto",
            default(value = "certificate"),
            exposable
        )]
        auth_method: C8yAuthMethod,

        /// The path where Cumulocity username/password are stored
        ///
        /// The value must be the path of the credentials file.
        #[tedge_config(
            example = "/etc/tedge/credentials.toml",
            default(from_config_dir = "credentials.toml")
        )]
        credentials_path: AbsolutePath,

        /// Set of MQTT topics the Cumulocity mapper should subscribe to
        #[tedge_config(
            example = "te/+/+/+/+/a/+,te/+/+/+/+/m/+,te/+/+/+/+/m/+/meta,te/+/+/+/+/e/+",
            default(value = "te/+/+/+/+,te/+/+/+/+/twin/+,te/+/+/+/+/m/+,te/+/+/+/+/m/+/meta,te/+/+/+/+/e/+,te/+/+/+/+/a/+,te/+/+/+/+/status/health"),
            exposable
        )]
        topics: TemplatesSet,
    }
}
