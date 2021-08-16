use structopt::StructOpt;

#[derive(StructOpt)]
struct PluginCli {
    #[structopt(subcommand)]
    operation: PluginOp,
}

#[derive(StructOpt)]
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
    let apt = PluginCli::from_args();

    match apt.operation {
        PluginOp::List {} | PluginOp::Prepare | PluginOp::Finalize => process_call_with_file(),

        PluginOp::Install {
            module: _,
            version: _,
        }
        | PluginOp::Remove {
            module: _,
            version: _,
        } => process_call_with_file(),
    };
}

fn process_call_with_file() {
    // When plugins are called by the agent they are forced to use `/tmp` as cwd.
    // If called not by the agent they will use users cwd, unless specifically told not to
    // and therefore `.tedge_dummy_plugin` directory should exist in that cwd.
    let tedge_dummy_path = std::env::current_dir()
        .unwrap()
        .join(".tedge_dummy_plugin/");

    // List all the files from `.tedge_dummy_plugin` and use them as source of output.
    // The file should be name as per following scheme:
    // <dummy_name>.<desired_exit_code>
    // The file contents should be exactly as expected stdout response.
    let mut list = std::fs::read_dir(tedge_dummy_path).unwrap();
    let data_path = list.next().unwrap().unwrap().path();
    let file = data_path.extension().unwrap().to_str().unwrap().to_string();

    // Parse .<exitcode> from the filename.
    let exit_code = file.parse::<i32>().unwrap();

    // Read content of the file as output of the plugin.
    let dummy_data = std::fs::read(data_path).unwrap();
    let dummy = String::from_utf8(dummy_data).unwrap();

    println!("{}", dummy);

    std::process::exit(exit_code);
}
