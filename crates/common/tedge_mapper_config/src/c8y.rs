//! The schema of a Cumulocity mapper's `mapper.toml`, matching the `c8y`
//! table of tedge_config

use std::net::IpAddr;
use tedge_config_engine::*;

use crate::absolute_path::AbsolutePath;
use crate::auto_flag::AutoFlag;
use crate::auto_log_upload::AutoLogUpload;
use crate::c8y_auth_method::C8yAuthMethod;
use crate::cloud_type::CloudType;
use crate::connect_url::ConnectUrl;
use crate::mqtt_payload_limit::MqttPayloadLimit;
use crate::seconds::SecondsOrHumanTime;
use crate::software_management_api_flag::SoftwareManagementApiFlag;
use crate::templates_set::TemplatesSet;
use crate::topic_prefix::TopicPrefix;

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

        proxy: {
            bind: {
                /// The IP address local Cumulocity HTTP proxy binds to
                #[tedge_config(example = "127.0.0.1", default(value = "127.0.0.1"))]
                address: IpAddr,

                /// The port local Cumulocity HTTP proxy binds to
                #[tedge_config(example = "8001", default(value = "8001"))]
                port: u16,
            },

            client: {
                /// The address of the host on which the local Cumulocity HTTP Proxy is running, used by the Cumulocity
                /// mapper.
                #[tedge_config(
                    default(value = "127.0.0.1"),
                    example = "127.0.0.1",
                    example = "192.168.1.2",
                    example = "tedge-hostname",
                    exposable
                )]
                host: String,

                /// The port number on the remote host on which the local Cumulocity HTTP Proxy is running, used by the
                /// Cumulocity mapper.
                #[tedge_config(example = "8001", default(from_key = "proxy.bind.port"), exposable)]
                port: u16,
            },

            /// The file that will be used as the server certificate for the Cumulocity proxy
            #[tedge_config(example = "/etc/tedge/device-certs/c8y_proxy_certificate.pem")]
            cert_path: AbsolutePath,

            /// The file that will be used as the server private key for the Cumulocity proxy
            #[tedge_config(example = "/etc/tedge/device-certs/c8y_proxy_key.pem")]
            key_path: AbsolutePath,

            /// Path to a file containing the PEM encoded CA certificates that are
            /// trusted when checking incoming client certificates for the Cumulocity Proxy
            #[tedge_config(example = "/etc/ssl/certs")]
            ca_path: AbsolutePath,
        },

        bridge: {
            include: {
                /// Set the bridge local clean session flag (this requires mosquitto >= 2.0.0)
                ///
                /// If set to 'auto', this cleans the local session accordingly the detected version of mosquitto.
                #[tedge_config(example = "auto", default(value = "auto"))]
                local_cleansession: AutoFlag,
            },

            /// The topic prefix that will be used for the bridge MQTT topic. For instance,
            /// if this is set to "c8y", then messages published to `c8y/s/us` will be
            /// forwarded to Cumulocity on the `s/us` topic
            #[tedge_config(example = "c8y", default(value = "c8y"), exposable)]
            topic_prefix: TopicPrefix,

            /// The amount of time after which the bridge should send a ping if no other traffic has occurred
            #[tedge_config(example = "60s", default(value = "60s"))]
            keepalive_interval: SecondsOrHumanTime,
        },

        entity_store: {
            /// Enable auto registration feature
            #[tedge_config(example = "true", default(value = "true"))]
            auto_register: bool,

            /// On a clean start, the whole state of the device, services and child-devices is resent to the cloud
            #[tedge_config(example = "true", default(value = "true"))]
            clean_start: bool,
        },

        software_management: {
            /// Switch legacy or advanced software management API to use. Value: legacy or advanced
            #[tedge_config(example = "advanced", default(value = "legacy"))]
            api: SoftwareManagementApiFlag,

            /// Enable publishing c8y_SupportedSoftwareTypes fragment to the c8y inventory API
            #[tedge_config(example = "true", default(value = "false"))]
            with_types: bool,
        },

        operations: {
            /// Auto-upload the operation log once it finishes.
            #[tedge_config(
                example = "always",
                example = "never",
                example = "on-failure",
                default(value = "on-failure")
            )]
            auto_log_upload: AutoLogUpload,
        },

        availability: {
            /// Enable sending heartbeat to Cumulocity periodically. If set to false, c8y_RequiredAvailability won't be sent
            #[tedge_config(example = "true", default(value = "true"))]
            enable: bool,

            /// Heartbeat interval to be sent to Cumulocity as c8y_RequiredAvailability.
            /// The value must be greater than 1 minute.
            /// If set to a lower value or 0, the device is considered in maintenance mode in the Cumulocity context.
            /// Details: https://cumulocity.com/docs/device-integration/fragment-library/#device-availability
            #[tedge_config(example = "60m", default(value = "60m"))]
            interval: SecondsOrHumanTime,
        },

        mqtt_service: {
            /// Whether to connect to the MQTT service endpoint or not
            #[tedge_config(example = "true", default(value = "false"), exposable)]
            enabled: bool,

            /// Set of MQTT topics the bridge should subscribe to on the Cumulocity MQTT service endpoint
            #[tedge_config(
                example = "incoming/topic,another/topic,test/topic",
                default(value = "[]"),
                exposable
            )]
            topics: TemplatesSet,
        },
    }
}
