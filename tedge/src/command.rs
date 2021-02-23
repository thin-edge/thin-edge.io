use crate::config;

/// A trait to be implemented by all tedge sub-commands.
///
/// In practice, an implementation will also:
/// * derive the `Structopt` trait to be parsed,
/// * and provide a specific error type implementing the `std::error:Error` trait
///
/// ```
/// #[derive(StructOpt, Debug)]
/// enum ConfigCmd {
///     /// Add new value (overwrite the value if the key exists).
///     Set { key: String, value: String },
///
///     /// Get value.
///     Get { key: String },
/// }
///
/// impl Command for ConfigCmd {
///     fn description(&self) -> String {
///        match self {
///            ConfigCmd::Set { key, value } => format!("set the parameter '{}' to the '{}'", key, value),
///            ConfigCmd::Get { key } => format!("get the value of the parameter '{}'", key),
///        }
///     }
///
///     // dummy implementation which always return an error
///     fn run(&self, _verbose: u8) -> Result<(), anyhow::Error> {
///        match self {
///            ConfigCmd::Set { key, value: _ },
///            ConfigCmd::Get { key } => UnknownKey{key},
///        }
///     }
/// }
///
/// #[derive(thiserror::Error, Debug)]
/// pub enum ConfigError {
///     #[error("Not a Thin Edge property: {key:?}")]
///     UnknownKey{key: String},
/// }
/// ```
pub trait Command {
    /// Display that command to the user, telling what will be done.
    ///
    /// This description is displayed to the end user in case of an error, to give the context of that error.
    fn description(&self) -> String;

    /// Execute this command.
    ///
    /// The simplest way to implement a specific `anyhow::Error` type is to derive the `thiserror::Error`.
    /// Doing so, the command specific error type implements `Into<anyhow::Error>`
    /// and such errors can then be returned with no explicit conversion from the `run()` method.
    ///
    /// ```
    /// #[derive(thiserror::Error, Debug)]
    /// pub enum ConfigError {
    ///     #[error("Not a Thin Edge property: {key:?}")]
    ///     UnknownKey{key: String},
    /// }
    /// ```
    fn execute(&self, verbose: u8) -> Result<(), anyhow::Error>;

    fn into_boxed(self) -> Box<dyn Command>
    where
        Self: Sized + 'static,
    {
        Box::new(self)
    }
}

pub trait BuildCommand {
    fn build_command(
        self,
        config: config::TEdgeConfig,
    ) -> Result<Box<dyn Command>, config::ConfigError>;
}

/// Return the value provided on the command line,
/// or, if not set, return the value stored in the config
/// or, if not found, return an error asking for the missing value.
///
/// ```
/// let path = param_config_or_default!(cert_path, tedge.device.cert_path, "device.cert.path");
/// ```
#[macro_export]
macro_rules! param_config_or_default {
    ($( $param:ident ).*, $( $config:ident ).*, $key:expr) => {
         $( $param ).* .as_ref()
         .or( $( $config ).*.as_ref())
         .map(|str| str.to_string())
         .ok_or_else(|| ConfigError::ConfigNotSet{key:String::from($key)});
    }
}
