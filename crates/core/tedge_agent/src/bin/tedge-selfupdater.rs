
use clap::Parser;
use std::process::{Command, ExitStatus, Stdio};

#[derive(Parser)]
struct AptCli {
    #[clap(subcommand)]
    operation: PluginOp,
}

#[derive(clap::Subcommand)]
pub enum PluginOp {
    /// Install or remove multiple modules at once
    UpdateList {
        #[clap(long = "--plugin-name")]
        plugin_name: String,
    },

}

// TODO: adapt enum below for self-updater purpose
#[derive(thiserror::Error, Debug)]
pub enum InternalError {
    #[error("Fail to run `{cmd}`: {from}")]
    ExecError { cmd: String, from: std::io::Error },

    #[error(transparent)]
    FromIo(#[from] std::io::Error),

    #[error(transparent)]
    FromUtf8(#[from] std::string::FromUtf8Error),

    #[error("Parsing Debian package failed for `{file}`")]
    ParsingError { file: String },

    #[error("Validation of {package} failed with version mismatch. Installed version: {installed}, Expected version: {expected}")]
    VersionMismatch {
        package: String,
        installed: String,
        expected: String,
    },
}

impl InternalError {
    pub fn exec_error(cmd: impl Into<String>, from: std::io::Error) -> InternalError {
        InternalError::ExecError {
            cmd: cmd.into(),
            from,
        }
    }
}

fn run(operation: PluginOp) -> Result<ExitStatus, InternalError> {
    match operation {
        PluginOp::UpdateList {
            plugin_name,
        } => {

            // identify all clouds
            let mut clouds : Vec<String> = Vec::new();

            // TODO: check for existing bridges 
            clouds.push("c8y".to_string());
            /*
            foreach b in "/etc/tedge/mosquitto-conf\/\*-bridge.conf"
              cloud += [ b ]
            */

            // disconnect all clouds
            // TODO: clone() below potentially not needed.
            for cloud in clouds.clone().into_iter() {
                // TODO: what when some get disconnected but others fail!?
                println!("Disconnecting {}", cloud);
                run_cmd(
                    "tedge",
                    &format!("disconnect {}", cloud),
                )?;
            }

            // exec plugin and forward STDIN
            println!("plugin_name 22 update-list");
            let status = Command::new(plugin_name)
                .args(vec!["update-list"])
                //.stdin(Stdio::piped())
                //.stdout(Stdio::piped())
                .status()
                /*.map_err(|err| InternalError::exec_error("apt-get", err))*/?;


            // TODO: handle error
            store_exitcode(status_to_exitcode(Ok(status)));

            // connect all clouds
            for cloud in clouds.into_iter() {
                println!("Connectiong {}", cloud);
                run_cmd(
                    "tedge",
                    &format!("connect {}", cloud),
                )?;
            }

            return Ok(status);
        }

    };
}


fn run_cmd(cmd: &str, args: &str) -> Result<ExitStatus, InternalError> {
    let args: Vec<&str> = args.split_whitespace().collect();
    let status = Command::new(cmd)
        .args(args)
        .stdin(Stdio::null())
        .status()
        /*.map_err(|err| InternalError::exec_error(cmd, err))*/?;
    Ok(status)
}

fn store_exitcode(exit_code: i32) -> std::io::Result<()> {
    let filepath = "/var/run/tedge_update";
    let filename = "selfupdate-result";

    // TODO: check existence of folder before creating 
    println!("Storing final result ({}) in {}", exit_code, format!("{}/{}", filepath, filename)); 
    std::fs::create_dir(filepath);
    std::fs::write(format!("{}/{}", filepath, filename), exit_code.to_string())?;
    Ok(())
}

fn status_to_exitcode(status : Result<ExitStatus, InternalError>) -> i32 {
    let exit_code;

    match status {
        Ok(status) if status.success() => {
            exit_code = 0;
        }

        Ok(status) => {
            if status.code().is_some() {
                exit_code = 2;
            } else {
                eprintln!("Interrupted by a signal!");
                exit_code = 4;
            }
        }

        Err(err) => {
            eprintln!("ERROR: {}", err);
            exit_code = 5;
        }
    }
    exit_code
}

fn main() {

    // On usage error, the process exits with a status code of 1
    let apt = AptCli::parse();

    run(apt.operation);
}

