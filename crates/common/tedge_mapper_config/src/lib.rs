#![forbid(unsafe_code)]

mod absolute_path;
mod auth_method;
mod c8y;
mod c8y_auth_method;
mod cert;
mod cloud_schema;
mod cloud_type;
mod connect_url;
mod cryptoki;
mod root;
mod schema;
mod seconds;
mod templates_set;
mod tls_enable;

pub use absolute_path::AbsolutePath;
pub use auth_method::AuthMethod;
pub use c8y::C8yMapperConfig;
pub use c8y::C8yMapperConfigDto;
pub use c8y_auth_method::C8yAuthMethod;
pub use cert::certificate_common_name;
pub use cloud_schema::CloudSchema;
pub use cloud_type::CloudType;
pub use connect_url::ConnectUrl;
pub use cryptoki::Cryptoki;
pub use root::RootConfig;
pub use root::RootConfigDto;
pub use schema::builtin_mapper_entries;
pub use schema::extract_builtin_mapper_name;
pub use schema::is_builtin_mapper_name;
pub use schema::load_federated_config;
pub use schema::mapper_key_from_dir_key;
pub use schema::mount_builtin_mapper;
pub use schema::MapperConfig;
pub use schema::MapperConfigDto;
pub use schema::BUILTIN_MAPPER_NAMES;
pub use seconds::SecondsOrHumanTime;
pub use templates_set::TemplatesSet;
pub use tls_enable::TlsEnable;

#[cfg(test)]
mod legacy_parity_tests;
#[cfg(test)]
mod tests;
