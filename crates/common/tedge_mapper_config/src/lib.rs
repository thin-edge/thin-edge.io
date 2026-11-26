#![forbid(unsafe_code)]

mod absolute_path;
mod auth_method;
mod auto_flag;
mod auto_log_upload;
mod c8y;
mod c8y_auth_method;
mod cert;
mod cloud_schema;
mod cloud_type;
mod cloud_type_change;
mod connect_url;
mod cryptoki;
mod host_port;
mod ip_address;
mod mqtt_payload_limit;
mod root;
mod schema;
mod seconds;
mod software_management_api_flag;
mod templates_set;
mod tls_enable;
mod topic_prefix;

pub use absolute_path::AbsolutePath;
pub use auth_method::AuthMethod;
pub use auto_flag::AutoFlag;
pub use auto_log_upload::AutoLogUpload;
pub use c8y::C8yMapperConfig;
pub use c8y_auth_method::C8yAuthMethod;
pub use cloud_type::CloudType;
pub use connect_url::ConnectUrl;
pub use cryptoki::Cryptoki;
pub use host_port::HostPort;
pub use host_port::HTTPS_PORT;
pub use host_port::MQTT_TLS_PORT;
pub use ip_address::IpAddress;
pub use mqtt_payload_limit::MqttPayloadLimit;
pub use root::RootConfig;
pub use schema::builtin_mapper_entries;
pub use schema::extract_builtin_mapper_name;
pub use schema::is_builtin_mapper_name;
pub use schema::load_federated_config;
pub use schema::mapper_key_from_dir_key;
pub use schema::mount_builtin_mapper;
pub use schema::mutate_mapper_key;
pub use schema::MapperConfig;
pub use schema::MapperConfigDto;
pub use schema::BUILTIN_MAPPER_NAMES;
pub use seconds::SecondsOrHumanTime;
pub use software_management_api_flag::SoftwareManagementApiFlag;
pub use templates_set::TemplatesSet;
pub use tls_enable::TlsEnable;
pub use topic_prefix::TopicPrefix;

#[cfg(test)]
mod legacy_parity_tests;
#[cfg(test)]
mod tests;
