//! Mapping of the Cumulocity `c8y_Command` operation to the thin-edge `shell_execute` command.
//!
//! The mapping is not hard-coded in the mapper, but provided by an operation template
//! deployed in the c8y operations directory, so it can be customized by the user.

use tedge_utils::paths::ManagedDir;
use tedge_utils::paths::PathsError;

/// The thin-edge command behind the Cumulocity `c8y_Command` operation
pub const SHELL_EXECUTE_OPERATION: &str = "shell_execute";

/// The name of the operation template mapping `c8y_Command` to the `shell_execute` command
pub const SHELL_EXECUTE_TEMPLATE_NAME: &str = "c8y_Command.template";

/// The built-in operation template mapping `c8y_Command` to the `shell_execute` command
pub const SHELL_EXECUTE_TEMPLATE: &str = include_str!("resources/c8y_Command.template");

/// Deploy the operation template mapping `c8y_Command` to the `shell_execute` command.
///
/// An existing template is never overwritten, as the user might have customized it.
pub async fn deploy_operation_template(ops_dir: &ManagedDir) -> Result<(), PathsError> {
    ops_dir
        .file(SHELL_EXECUTE_TEMPLATE_NAME)?
        .create_if_missing(SHELL_EXECUTE_TEMPLATE)
        .await
}
