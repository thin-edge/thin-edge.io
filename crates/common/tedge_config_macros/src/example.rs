//! An example invocation of [define_tedge_config] to demonstrate what
//! it expands to in `cargo doc` output.
use super::*;
use camino::Utf8PathBuf;
use std::path::PathBuf;

#[derive(thiserror::Error, Debug)]
/// *Not macro generated!* An error that can be encountered when reading values
/// from the configuration
///
/// As custom logic (e.g. for read-only values) needs to interact with this,
/// this is left to the consuming module to define. It must include a case with
/// `#[from] ConfigNotSet`, for instance:
///
/// ```
/// use tedge_config_macros::ConfigNotSet;
///
/// #[derive(thiserror::Error, Debug)]
/// pub enum ReadError {
///     #[error(transparent)]
///     ConfigNotSet(#[from] ConfigNotSet),
///     
///     // Add more cases, such as errors from inferring the device id, here
/// }
///
/// ```
pub enum ReadError {
    #[error(transparent)]
    ConfigNotSet(#[from] ConfigNotSet),
}

/// A trait defined to conveniently emit [ReadError]s from [OptionalConfig]
/// values
///
/// Since this depends on [ReadError], this is not macro generated.
///
/// ```
/// # #[derive(thiserror::Error, Debug)]
/// # pub enum ReadError {
/// #     #[error(transparent)]
/// #     ConfigNotSet(#[from] ConfigNotSet),
/// # }
///
/// # pub trait OptionalConfigError<T> {
/// #     fn or_err(&self) -> Result<&T, ReadError>;
/// # }
///
/// # impl<T> OptionalConfigError<T> for OptionalConfig<T> {
/// #     fn or_err(&self) -> Result<&T, ReadError> {
/// #         self.or_config_not_set().map_err(ReadError::from)
/// #     }
/// # }
///
/// use tedge_config_macros::*;
///
/// define_tedge_config! {
///     c8y: {
///         url: ConnectUrl,
///     }
/// }
///
/// fn connect_to_c8y(reader: &TEdgeConfigReader) -> Result<(), ReadError> {
///     // If we fail here, the error message will tell the user that 'c8y.url' is unset
///     let url = reader.c8y.url.or_err()?;
///     println!("Connecting to Cumulocity: {url}");
///     Ok(())
/// }
/// ```
pub trait OptionalConfigError<T> {
    fn or_err(&self) -> Result<&T, ReadError>;
}

impl<T> OptionalConfigError<T> for OptionalConfig<T> {
    fn or_err(&self) -> Result<&T, ReadError> {
        self.or_config_not_set().map_err(ReadError::from)
    }
}

define_tedge_config! {
    /// The device settings. Group doc comments are not used in tedge config, but they are copied to the Reader and DTO.
    device: {
        /// The root certificate path
        #[doku(as = "PathBuf")]
        root_cert_path: Utf8PathBuf,

        #[doku(as = "PathBuf")]
        #[tedge_config(default(from_optional_key = "device.root_cert_path"))]
        root_cert_path2: Utf8PathBuf,

        #[tedge_config(rename = "type")]
        ty: String,
    }
}
