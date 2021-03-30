use crate::cli::config::config_keys::*;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
pub enum ConfigCmd {
    /// Get the value of the provided configuration key
    Get {
        /// Configuration key.
        #[structopt(help = HELP_VALID_READ_ONLY_KEYS)]
        key: ReadOnlyConfigKey,
    },

    /// Set or update the provided configuration key with the given value
    Set {
        /// Configuration key.
        #[structopt(help = HELP_VALID_READ_WRITE_KEYS)]
        key: ReadWriteConfigKey,

        /// Configuration value.
        value: String,
    },

    /// Unset the provided configuration key
    Unset {
        /// Configuration key.
        #[structopt(help = HELP_VALID_UNSETTABLE_KEYS)]
        key: UnsettableConfigKey,
    },

    /// Print the configuration keys and their values
    List {
        /// Prints all the configuration keys, even those without a configured value
        #[structopt(long = "all")]
        is_all: bool,

        /// Prints all keys and descriptions with example values
        #[structopt(long = "doc")]
        is_doc: bool,
    },
}
