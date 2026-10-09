#![forbid(unsafe_code)]

mod auth_method;
mod schema;
mod seconds;

pub use auth_method::AuthMethod;
pub use schema::builtin_mapper_entries;
pub use schema::extract_builtin_mapper_name;
pub use schema::is_builtin_mapper_name;
pub use schema::load_federated_config;
pub use schema::BUILTIN_MAPPER_NAMES;
pub use schema::MapperConfig;
pub use schema::MapperConfigDto;
pub use schema::RootStubConfig;
pub use schema::RootStubConfigDto;
pub use seconds::SecondsOrHumanTime;

#[cfg(test)]
mod tests;
