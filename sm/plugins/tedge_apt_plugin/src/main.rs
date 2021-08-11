use std::process::{Command, ExitStatus, Stdio};
use structopt::StructOpt;

#[derive(StructOpt)]
struct AptCli {
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
        #[structopt(short = "v", long = "--module-version")]
        version: Option<String>,
    },

    /// Uninstall a module
    Remove {
        module: String,
        #[structopt(short = "v", long = "--module-version")]
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

    #[error("Package with the specified `{version}` version not installed")]
    PackageNotInstalled { module: String, version: String },
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
    let status = match operation {
        PluginOp::List {} => {
            let apt = Command::new("apt")
                .args(vec!["--manual-installed", "list"])
                .stdout(Stdio::piped()) // To pipe apt.stdout into awk.stdin
                .spawn()
                .map_err(|err| InternalError::exec_error("apt", err))?;

            // apt output    = openssl/focal-security,now 1.1.1f-1ubuntu2.3 amd64 [installed]
            // awk -F '[/ ]' =   $1   ^       $2         ^   $3            ^   $4
            // awk print     =   name ^                  ^   version       ^
            let status = Command::new("awk")
                .args(vec![
                    "-F",
                    "[/ ]",
                    r#"{if ($1 != "Listing...") { print "{\"name\":\""$1"\",\"version\":\""$3"\"}"}}"#,
                ])
                .stdin(apt.stdout.unwrap()) // Cannot panic: apt.stdout has been set
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

        PluginOp::Remove { module, version } => {
            if let Some(version) = version {
                // check the version mentioned present or not
                if check_if_the_module_with_version_installed(&module, &version)? {
                    run_cmd("apt-get", &format!("remove --quiet --yes {}", module))?
                } else {
                    return Err(InternalError::PackageNotInstalled { module, version });
                }
            } else {
                run_cmd("apt-get", &format!("remove --quiet --yes {}", module))?
            }
        }

        PluginOp::Prepare => run_cmd("apt-get", &format!("update --quiet --yes"))?,

        PluginOp::Finalize => run_cmd("apt-get", &format!("auto-remove --quiet --yes"))?,
    };

    Ok(status)
}

fn run_cmd(cmd: &str, args: &str) -> Result<ExitStatus, InternalError> {
    let args: Vec<&str> = args.split_whitespace().collect();
    let status = Command::new(cmd)
        .args(args)
        .stdin(Stdio::null())
        .status()
        .map_err(|err| InternalError::exec_error(cmd, err))?;
    Ok(status)
}

fn check_if_the_module_with_version_installed(
    module: &str,
    version: &str,
) -> Result<bool, InternalError> {
    let cmd = "dpkg";
    let cmd_args = &format!("-s {}", module);
    let args: Vec<&str> = cmd_args.split_whitespace().collect();
    let mut query_package_output_child = Command::new(cmd)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|err| InternalError::exec_error(cmd, err))?;

    if let Some(query_package_output) = query_package_output_child.stdout.take() {
        let status = Command::new("grep")
            .arg("-i")
            .arg(version)
            .stdin(query_package_output)
            .stdout(Stdio::piped())
            .status()
            .map_err(|err| InternalError::exec_error(cmd, err))?;
        query_package_output_child
            .wait()
            .map_err(|err| InternalError::exec_error(cmd, err))?;

        if status.success() {
            return Ok(true);
        } else {
            return Ok(false);
        }
    }
    return Ok(false);
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
