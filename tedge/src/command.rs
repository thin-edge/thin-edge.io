use crate::config;
use crate::utils::users::UserManager;

/// A trait to be implemented by all tedge sub-commands.
///
/// A command encapsulates all the required parameters and provides an `execute()` method
/// to trigger the execution, plus a `description` of what is done by that command.
///
/// ```
/// struct SayHello {
///     name: String,
/// };
///
/// impl Command for SayHello {
///     fn description(&self) -> String {
///        format!("say hello to '{}'", name),
///     }
///
///     fn execute(&self) -> Result<(), anyhow::Error> {
///        println!("Hello {}!", name};
///        Ok(())
///     }
/// }
/// ```
///
/// If a command needs some context, say the tedge configuration,
/// this context can be provided to the command.
///
/// ```
/// struct GetConfigKey {
///     config: TEdgeConfig,
///     key: String,
/// };
///
/// impl Command for GetConfigKey {
///     fn description(&self) -> String {
///        format!("get the value of the configuration key '{}'", self.key),
///     }
///
///     fn execute(&self) -> Result<(), anyhow::Error> {
///        match self.config.get_config_value(self.key)? {
///             Some(value) => println!("{}", value),
///             None => eprintln!("The configuration key `{}` is not set", self.key),
///        }
///        Ok(())
///     }
/// }
/// ```
///
/// No specific support is provided to a command that needs to update the configuration.
/// The simplest is to acquire a mutable config using `TEdgeConfig::from_default_config()`.
///
/// ```
/// struct SetConfigKey {
///     key: String,
///     value: String,
/// };
///
/// impl Command for SetConfigKey {
///     fn description(&self) -> String {
///        format!("set the value of the configuration key '{}' to '{}'", self.key, self.value),
///     }
///
///     fn execute(&self) -> Result<(), anyhow::Error> {
///        let mut config = TEdgeConfig::from_default_config()?;
///        config.set_config_value(self.key, self.value)?;
///        let _ = config.write_to_default_config()?;
///        Ok(())
///     }
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
    fn execute(&self, user_manager: UserManager) -> Result<(), anyhow::Error>;

    fn into_boxed(self) -> Box<dyn Command>
    where
        Self: Sized + 'static,
    {
        Box::new(self)
    }
}

/// A trait implemented by the tedge subcommands to build the actual command
/// using a combination of the parameters provided on the command line
/// and those from the configuration.
///
/// In practice, an implementation will also derives the `Structopt` trait.
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
/// impl BuildCommand for ConfigCmd {
///     fn build_command(self, config: TEdgeConfig) -> Result<Box<dyn Command>, ConfigError> {
///        match self {
///            ConfigCmd::Set { key, value } => SetConfigKey {
///                config,
///                key,
///                value,
///            },
///            ConfigCmd::Get { key } => GetConfigKey {
///                config,
///                key,
///            },
///        }
///     }
/// }
/// ```
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
