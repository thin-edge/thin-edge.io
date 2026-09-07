#![forbid(unsafe_code)]

mod auth_method;
mod schema;
mod seconds;

pub use auth_method::AuthMethod;
pub use schema::load_federated_config;
pub use seconds::SecondsOrHumanTime;
pub use schema::MapperConfig;
pub use schema::MapperConfigDto;
pub use schema::RootStubConfig;
pub use schema::RootStubConfigDto;

#[cfg(test)]
mod tests;
