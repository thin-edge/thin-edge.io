use crate::system_command::Role;

/// Representation of a system command to be run by a `SystemCommandRunner`.
///
/// The `SystemCommand` does not allow for very complex pipeline constructions by purpose.
///
/// Note: We are using `String`s here and not `OsString`. We want the `SystemCommand` to be easily
/// inspectable or serializable, potentially send via IPC.
///
#[derive(Debug)]
pub struct SystemCommand {
    /// The binary to be executed.
    pub program: String,

    /// The arguments to the binary.
    pub args: Vec<String>,

    /// The role the command is executed as. If `None`, the default role is used.
    pub role: Option<Role>,
}

impl SystemCommand {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            role: None,
        }
    }

    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn role(self, role: Option<Role>) -> Self {
        Self { role, ..self }
    }
}
