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

    /// Capture stdout. If `None`, no output is captured. If `Some` the maximum buffer size is
    /// given (if > 0). If negative, buffer size is unbounded.
    pub capture_output: Option<isize>,

    /// Capture stderr. If `None`, no error output is captured. If `Some` the maximum buffer size
    /// is given (if > 0). If negative, buffer size is unbounded.
    pub capture_error: Option<isize>,

    /// Timeout. Upper time limit until command termination.
    pub timeout: Option<std::time::Duration>,
}

impl SystemCommand {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            role: None,
            capture_output: None,
            capture_error: None,
            timeout: None,
        }
    }

    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn role(self, role: Option<Role>) -> Self {
        Self { role, ..self }
    }

    pub fn capture_output(self, capture_output: Option<isize>) -> Self {
        Self {
            capture_output,
            ..self
        }
    }

    pub fn capture_error(self, capture_error: Option<isize>) -> Self {
        Self {
            capture_error,
            ..self
        }
    }

    pub fn timeout(self, timeout: Option<std::time::Duration>) -> Self {
        Self { timeout, ..self }
    }
}
