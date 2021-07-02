use std::process::{Command, Stdio};
use structopt::StructOpt;

#[derive(StructOpt)]
struct AptCli {
    #[structopt(subcommand)]
    operation: PluginOp,
}

#[derive(StructOpt)]
pub enum PluginOp {
    /// List all the installed modules
    List {},

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
    ExecError {
        cmd: String,
        from: std::io::Error,
    },
}

impl InternalError {
    pub fn exec_error(cmd: impl Into<String>, from: std::io::Error) -> InternalError {
        InternalError::ExecError { cmd: cmd.into(), from }
    }
}

fn run(operation: PluginOp) -> Result<std::process::ExitStatus, InternalError> {
    let status = match operation {
        PluginOp::List {} => {
            let cmd = Command::new("apt")
                .args(vec!["--installed", "list"])
                .stdout(Stdio::piped())
                .spawn()
                .map_err(|err| InternalError::exec_error("apt", err))?;

            let cmd2 = Command::new("grep")
                .args(vec!["-v", "automatic"])
                .stdin(cmd.stdout.unwrap())
                .stdout(Stdio::piped())
                .spawn()
                .map_err(|err| InternalError::exec_error("grep", err))?;

            let status = Command::new("awk")
                .arg(r#"{print "{\"name\":\""$1"\",\"version\":\""$2"\"}"}"#)
                .stdin(cmd2.stdout.unwrap())
                .status()
                .map_err(|err| InternalError::exec_error("awk", err))?;

            status
        }

        PluginOp::Install { module, version } => {
            if let Some(version) = version {
                run_cmd(
                    "apt-get",
                    &format!("install --quiet --yes {}={}", module, version),
                )?
            } else {
                run_cmd("apt-get", &format!("install --quiet --yes {}", module))?
            }
        }

        PluginOp::Remove {
            module,
            version: _unused,
        } => run_cmd("apt-get", &format!("remove --quiet --yes {}", module))?,

        PluginOp::Prepare => run_cmd(
            "apt-get",
            &format!("update --quiet --yes"),
        )?,

        PluginOp::Finalize => run_cmd(
            "apt-get",
            &format!("auto-remove --quiet --yes"),
        )?,

    };

    Ok(status)
}

fn run_cmd(cmd: &str, args: &str) -> Result<std::process::ExitStatus, InternalError> {
    let args: Vec<&str> = args.split_whitespace().collect();
    let status = Command::new(cmd)
        .args(args)
        .stdin(Stdio::null())
        .status()
        .map_err(|err| InternalError::exec_error(cmd, err))?;
    Ok(status)
}

fn main() {
    // On usage error, the process exits with a status code of 1
    let apt = AptCli::from_args();

    match run(apt.operation) {
        Ok(status) if status.success() => {
            std::process::exit(0);
        }

        Ok(status) => {
            if let Some(_) = status.code() {
                std::process::exit(2);
            } else {
                eprintln!("Interrupted by a signal!");
                std::process::exit(4);
            }
        }

        Err(err) => {
            eprintln!("ERROR: {}", err);
            std::process::exit(5);
        }
    }
}
