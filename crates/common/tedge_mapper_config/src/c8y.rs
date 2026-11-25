//! The schema of a Cumulocity mapper's `mapper.toml`, matching the `c8y`
//! table of tedge_config

use tedge_config_engine::*;

use crate::absolute_path::AbsolutePath;
use crate::c8y_auth_method::C8yAuthMethod;
use crate::cloud_type::CloudType;
use crate::connect_url::ConnectUrl;
use crate::mqtt_payload_limit::MqttPayloadLimit;
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

        smartrest: {
            /// Set of SmartREST template IDs the device should subscribe to
            #[tedge_config(example = "templateId1,templateId2", default(value = "[]"), exposable)]
            templates: TemplatesSet,

            /// Switch using 501-503 (without operation ID) or 504-506 (with operation ID) SmartREST messages for operation status update
            #[tedge_config(example = "true", default(value = "true"))]
            use_operation_id: bool,

            child_device: {
                /// Attach the c8y_IsDevice fragment to child devices on creation
                #[tedge_config(example = "false", default(value = "false"))]
                create_with_device_marker: bool,
            },
        },

        smartrest1: {
            /// Set of SmartREST 1.0 template IDs the device should subscribe to
            #[tedge_config(example = "templateId1,templateId2", default(value = "[]"), exposable)]
            templates: TemplatesSet,
        },

        enable: {
            /// Enable log_upload feature
            #[tedge_config(example = "true", default(value = "true"), exposable, deprecated_key = "enable.log_management")]
            log_upload: bool,

            /// Enable config_snapshot feature
            #[tedge_config(example = "true", default(value = "true"), exposable)]
            config_snapshot: bool,

            /// Enable config_update feature
            #[tedge_config(example = "true", default(value = "true"), exposable)]
            config_update: bool,

            /// Enable firmware_update feature
            #[tedge_config(example = "true", default(value = "true"), exposable)]
            firmware_update: bool,

            /// Enable device_profile feature
            #[tedge_config(example = "true", default(value = "true"), exposable)]
            device_profile: bool,

            /// Enable device restart feature
            #[tedge_config(example = "true", default(value = "true"), exposable)]
            device_restart: bool,

            /// Enable software update feature
            #[tedge_config(example = "true", default(value = "true"), exposable)]
            software_update: bool,

            /// Enable shell execute feature
            #[tedge_config(example = "true", default(value = "true"), exposable)]
            shell_execute: bool,

            /// Enable service control feature
            #[tedge_config(example = "true", default(value = "true"), exposable)]
            service_control: bool,
        },

        mapper: {
            mqtt: {
                /// The maximum message payload size that can be mapped to the cloud via MQTT
                #[tedge_config(example = "16184", default(value = "16184"), exposable)]
                max_payload_size: MqttPayloadLimit,
            },
        },
    }
}
