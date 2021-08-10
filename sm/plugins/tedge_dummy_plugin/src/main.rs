use structopt::StructOpt;

#[derive(StructOpt)]
struct PluginCli {
    #[structopt(subcommand)]
    _operation: PluginOp,
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
    let tedge_dummy_path = std::env::current_dir()
        .unwrap()
        .join(".tedge_dummy_plugin/");

    dbg!(&tedge_dummy_path);
    let mut list = std::fs::read_dir(tedge_dummy_path).unwrap();
    let data_path = list.next().unwrap().unwrap().path();
    let file = data_path.extension().unwrap().to_str().unwrap().to_string();

    let exit_code = file.parse::<i32>().unwrap();
    dbg!(exit_code);

    let dummy_data = std::fs::read(data_path).unwrap();

    let dummy = String::from_utf8(dummy_data).unwrap();
    println!("{}", dummy);

    std::process::exit(exit_code);
}
