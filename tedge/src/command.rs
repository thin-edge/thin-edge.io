/// A trait to be implemented by all tedge sub-commands.
pub trait Command {
    /// Display that command to the user, telling what will be done
    fn to_string(&self) -> String;

    /// Run this command
    fn run(&self, verbose: u8) -> Result<(), Box<dyn std::error::Error>>;
}
