
use clap::{crate_authors, crate_description, crate_name, crate_version, App, Arg, SubCommand};

/// Options specified on the command line.
pub struct Options {
    pub log_level: String,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            log_level: "Warn".to_string(),
        }
    }
}

impl Options {
    /// Build `Options` from command line arguments.
    pub fn new() -> Self {
        let version = crate_version!().to_owned();

        let mut options = Options::default();

        let matches = App::new(crate_name!())
            .version(version.as_str())
            .author(crate_authors!("\n"))
            .about(crate_description!())
            .subcommand(SubCommand::with_name("demo").about("It is to show how to use this cli"))
            .arg(
                Arg::with_name("v")
                    .short("v")
                    .multiple(true)
                    .help("Increases the level of verbosity (the max level is -vvv)"),
            )
            .get_matches();

        match matches.occurrences_of("v") {
            0 => {},
            1 => options.log_level = "Info".to_string(),
            2 => options.log_level = "Debug".to_string(),
            _ => options.log_level = "Trace".to_string(),
        }

        options
    }
}
