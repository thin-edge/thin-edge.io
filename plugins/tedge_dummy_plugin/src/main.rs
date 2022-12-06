use clap::Parser;

#[derive(Parser)]
struct PluginCli {
    #[clap(subcommand)]
    operation: PluginOp,
}

#[derive(clap::Subcommand)]
pub enum PluginOp {
    /// List all the installed modules
    List,

    /// Install a module
    Install {
        module: String,
        version: Option<String>,
    },

    /// Uninstall a module
    Remove {
        module: String,
        version: Option<String>,
    },

    /// Install or remove multiple modules at once
    UpdateList,

    /// Prepare a sequences of install/remove commands
    Prepare,

    /// Finalize a sequences of install/remove commands
    Finalize,
}

#[derive(thiserror::Error, Debug)]
pub enum InternalError {
    #[error("Fail to run `{cmd}`: {from}")]
    ExecError { cmd: String, from: std::io::Error },
}

impl InternalError {
    pub fn exec_error(cmd: impl Into<String>, from: std::io::Error) -> InternalError {
        InternalError::ExecError {
            cmd: cmd.into(),
            from,
        }
    }
}

fn main() {
    // Emulate plugin's API.
    let apt = PluginCli::parse();

    match apt.operation {
        PluginOp::List
        | PluginOp::Prepare
        | PluginOp::Finalize
        | PluginOp::UpdateList
        | PluginOp::Install { .. }
        | PluginOp::Remove { .. } => process_call_with_file(),
    };
}

fn process_call_with_file() {
    // When plugins are called by the agent they are forced to use `/tmp` as cwd.
    // If called not by the agent they will use users cwd, unless specifically told not to
    // and therefore `.tedge-dummy-plugin` directory should exist in that cwd.
    let tedge_dummy_path = std::env::current_dir()
        .unwrap()
        .join(".tedge-dummy-plugin/");

    // List all the files from `.tedge-dummy-plugin` and use them as source of output.
    // The file should be name as per following scheme:
    // <dummy_name>.<desired_exit_code>
    // The file contents should be exactly as expected stdout response.
    // The process will terminate with code 0 if there is no files available to parse.
    let mut list = match std::fs::read_dir(tedge_dummy_path) {
        Ok(read_dir) => read_dir,
        Err(_) => std::process::exit(0),
    };

    let data_path = if let Some(Ok(dir_entry)) = list.next() {
        dir_entry.path()
    } else {
        std::process::exit(0)
    };

    let file_extension = match data_path.extension() {
        Some(path) => match path.to_str() {
            Some(path_str) => path_str,
            None => std::process::exit(0),
        },
        None => std::process::exit(0),
    };

    // Parse .<exitcode> from the filename.
    if let Ok(exit_code) = file_extension.parse::<i32>() {
        // Read content of the file as output of the plugin.
        let dummy_data = std::fs::read(data_path).unwrap();
        let dummy = String::from_utf8(dummy_data).unwrap();

        println!("{}", dummy);
        std::process::exit(exit_code);
    };
}
