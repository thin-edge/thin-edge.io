use crate::system_services::*;
use std::sync::Arc;
use tedge_users::UserManager;

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
///     fn execute(&self) -> anyhow::Result<()> {
///        println!("Hello {}!", name};
///        Ok(())
///     }
/// }
/// ```
///
/// If a command needs some context, say the tedge configuration,
/// this context can be provided to the command struct.
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
///     fn execute(&self) -> anyhow::Result<()> {
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
///     fn execute(&self) -> anyhow::Result<()> {
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
    /// The errors returned by this method must be concrete `anyhow::Error` values.
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
    fn execute(&self) -> anyhow::Result<()>;

    /// Helper method to be used in the `BuildCommand` trait.
    ///
    /// The `BuildCommand::build_command()` method has to return a box around a new command.
    ///
    /// ```
    /// fn build_command(self, config: TEdgeConfig) -> Result<Box<dyn Command>, ConfigError> {
    ///     let cmd = GetConfigKey { config, key };
    ///     Ok(cmd.into_boxed())
    /// }
    /// ```
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
///     fn build_command(self, context: BuildContext) -> Result<Box<dyn Command>, ConfigError> {
///        let cmd = match self {
///            ConfigCmd::Set { key, value } => SetConfigKey {
///                config: context.config,
///                key,
///                value,
///            },
///            ConfigCmd::Get { key } => GetConfigKey {
///                config: context.config,
///                key,
///            },
///        }
///        Ok(cmd.into_boxed())
///     }
/// }
/// ```
pub trait BuildCommand {
    fn build_command(self, context: BuildContext) -> Result<Box<dyn Command>, crate::ConfigError>;
}

/// The context for `BuildCommand`
///
pub struct BuildContext {
    pub config_repository: tedge_config::TEdgeConfigRepository,
    pub config_location: tedge_config::TEdgeConfigLocation,
    pub service_manager: Arc<dyn SystemServiceManager>,
    pub user_manager: UserManager,
}
